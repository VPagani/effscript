//! Lowering `machine` declarations to `@typeonce/effect-machine`.

use efs_syntax::Span;
use efs_syntax::ast::*;
use efs_syntax::visit::referenced_names;

use crate::lower::Gen;

/// Names a transition callback receives from effect-machine.
const CONTEXT_NAMES: &[&str] = &[
    "event",
    "state",
    "root",
    "ancestors",
    "snapshot",
    "output",
    "error",
    "element",
    "input",
    "containingState",
    "self",
    "parent",
];

struct Ctx {
    /// Every state's full dotted path.
    paths: Vec<String>,
    /// Timers synthesized from `after`: (name, delay).
    timers: Vec<(String, Expr)>,
}

fn collect_paths(body: &StateBody, prefix: &str, out: &mut Vec<String>) {
    for s in &body.states {
        let path = if prefix.is_empty() { s.name.name.clone() } else { format!("{prefix}.{}", s.name.name) };
        out.push(path.clone());
        collect_paths(&s.body, &path, out);
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('.').map_or("", |(p, _)| p)
}

impl Gen<'_> {
    pub(crate) fn machine(&mut self, item: &Item, m: &Machine) {
        let export = if item.export { "export " } else { "" };
        let name = m.name.name.clone();
        let mut paths = Vec::new();
        collect_paths(&m.body, "", &mut paths);
        let mut ctx = Ctx { paths, timers: Vec::new() };
        let machine_rt = self.rt("Machine");

        // Topology.
        self.w.s(&format!("{export}const {name}State = {machine_rt}.state("));
        self.topology(m.parallel, false, &m.root_fields, &m.body);
        self.w.s(")");
        self.w.nl();

        // Events.
        self.w.s(&format!("{export}const {name}Events = {machine_rt}.events("));
        self.variant_fields_map(&m.events);
        self.w.s(")");
        self.w.nl();

        // Handlers are generated first so that `after` timers are known for `make`.
        let saved_out = std::mem::take(&mut self.w.out);
        let saved_segs = std::mem::take(&mut self.w.segs);
        let handler_start_len = saved_out.len();
        self.w.out = saved_out.clone();
        self.handlers(&mut ctx, "", m.parallel, &m.root_fields, &m.body, true);
        let handlers_text = self.w.out[handler_start_len..].to_string();
        let mut handler_segs = std::mem::replace(&mut self.w.segs, saved_segs);
        self.w.out = saved_out;

        self.w.s(&format!("{export}const "));
        self.w.v(m.name.span);
        self.w.s(&format!(" = {machine_rt}.make({{"));
        self.w.indent();
        self.w.nl();
        self.w.s(&format!("root: {name}State,"));
        if let Some(input) = &m.input {
            self.w.nl();
            self.w.s("input: ");
            self.schema_of(input);
            self.w.s(",");
        }
        self.w.nl();
        self.w.s(&format!("events: {name}Events"));
        for (kind, key) in
            [(SourceKind::Effects, "effects"), (SourceKind::Streams, "streams"), (SourceKind::Timers, "timers")]
        {
            let entries: Vec<&MachineSource> = m.sources.iter().filter(|s| s.kind == kind).collect();
            let synthesized = if kind == SourceKind::Timers { std::mem::take(&mut ctx.timers) } else { Vec::new() };
            if entries.is_empty() && synthesized.is_empty() {
                continue;
            }
            self.w.s(",");
            self.w.nl();
            self.w.s(&format!("{key}: {{"));
            self.w.indent();
            let mut first = true;
            for s in entries {
                if !first {
                    self.w.s(",");
                }
                first = false;
                self.w.nl();
                self.w.v(s.name.span);
                self.w.s(": ");
                self.expr_top(&s.value);
            }
            for (tname, delay) in &synthesized {
                if !first {
                    self.w.s(",");
                }
                first = false;
                self.w.nl();
                self.w.s(&format!("{tname}: "));
                self.expr_top(delay);
            }
            self.w.dedent();
            self.w.nl();
            self.w.s("}");
        }
        self.w.dedent();
        self.w.nl();
        self.w.s("}).handle(");
        // Splice the handler text in at the current position.
        let shift = self.w.pos() as i64 - handler_start_len as i64;
        self.w.out.push_str(&handlers_text);
        for s in &mut handler_segs {
            s.gen_start = (s.gen_start as i64 + shift) as u32;
        }
        self.w.segs.extend(handler_segs);
        self.w.s(")");
    }

    fn variant_fields_map(&mut self, variants: &[Variant]) {
        if variants.is_empty() {
            self.w.s("{}");
            return;
        }
        self.w.s("{");
        self.w.indent();
        for (i, v) in variants.iter().enumerate() {
            self.w.push_node(v.span);
            if i > 0 {
                self.w.s(",");
            }
            self.w.nl();
            self.w.v(v.name.span);
            self.w.s(": ");
            self.schema_struct_fields_inline(&v.fields);
            self.w.pop_node();
        }
        self.w.dedent();
        self.w.nl();
        self.w.s("}");
    }

    /// `{ a: Schema.String, b: Schema.Number }` on one line.
    pub(crate) fn schema_struct_fields_inline(&mut self, fields: &[SchemaField]) {
        if fields.is_empty() {
            self.w.s("{}");
            return;
        }
        self.w.s("{ ");
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                self.w.s(", ");
            }
            self.w.push_node(f.span);
            self.w.v(f.name.span);
            self.w.s(": ");
            self.schema_field_value(f);
            self.w.pop_node();
        }
        self.w.s(" }");
    }

    fn topology(&mut self, parallel: bool, is_final: bool, fields: &[SchemaField], body: &StateBody) {
        let mut entries = 0;
        let mut open = |g: &mut Self| {
            if entries == 0 {
                g.w.s("{ ");
            } else {
                g.w.s(", ");
            }
            entries += 1;
        };
        if parallel {
            open(self);
            self.w.s("type: \"parallel\"");
        } else if is_final {
            open(self);
            self.w.s("type: \"final\"");
        }
        if !fields.is_empty() {
            open(self);
            self.w.s("fields: ");
            self.schema_struct_fields_inline(fields);
        }
        if !body.states.is_empty() {
            open(self);
            self.w.s("states: {");
            self.w.indent();
            for (i, s) in body.states.iter().enumerate() {
                self.w.push_node(s.span);
                if i > 0 {
                    self.w.s(",");
                }
                self.w.nl();
                self.w.v(s.name.span);
                self.w.s(": ");
                self.topology(s.parallel, s.is_final, &s.fields, &s.body);
                self.w.pop_node();
            }
            self.w.dedent();
            self.w.nl();
            self.w.s("}");
        }
        self.w.s(if entries == 0 { "{}" } else { " }" });
    }

    fn resolve_target(&mut self, ctx: &Ctx, path: &[Ident], from: &str) -> String {
        let joined = path.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join(".");
        let span = path[0].span.to(path.last().unwrap().span);
        if ctx.paths.contains(&joined) {
            return joined;
        }
        let suffix = format!(".{joined}");
        let matches: Vec<&String> = ctx.paths.iter().filter(|p| p.ends_with(&suffix)).collect();
        if matches.len() == 1 {
            return matches[0].clone();
        }
        // Prefer the candidate closest to the current state.
        let mut scope = from.to_string();
        loop {
            let candidate = if scope.is_empty() { joined.clone() } else { format!("{scope}.{joined}") };
            if ctx.paths.contains(&candidate) {
                return candidate;
            }
            if scope.is_empty() {
                break;
            }
            scope = parent_of(&scope).to_string();
        }
        if matches.is_empty() {
            self.error(span, format!("unknown state `{joined}`"));
        } else {
            self.error(span, format!("`{joined}` is ambiguous; write the full path, such as `{}`", matches[0]));
        }
        joined
    }

    /// `({ a, b }) => ` for the context names `exprs` use.
    fn context_params(&mut self, exprs: &[&Expr], always: &[&str]) {
        let mut names: Vec<String> = always.iter().map(|s| s.to_string()).collect();
        for e in exprs {
            for n in referenced_names(e) {
                if CONTEXT_NAMES.contains(&n.as_str()) && !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        if names.is_empty() {
            self.w.s("() => ");
        } else {
            self.w.s(&format!("({{ {} }}) => ", names.join(", ")));
        }
    }

    fn uses_context(e: &Expr) -> bool {
        referenced_names(e).iter().any(|n| CONTEXT_NAMES.contains(&n.as_str()))
    }

    fn transition(&mut self, ctx: &Ctx, t: &Transition, guard: Option<&Expr>, from: &str) {
        let span = match t {
            Transition::Target { span, .. } | Transition::Update { span, .. } => *span,
            Transition::Stay(s) => *s,
        };
        self.w.push_node(span);
        self.w.s("{ ");
        match t {
            Transition::Stay(_) => self.w.s("stay: true"),
            Transition::Target { path, data, .. } => {
                let target = self.resolve_target(ctx, path, from);
                let tspan = path[0].span.to(path.last().unwrap().span);
                self.w.s("target: ");
                self.w.s_at(tspan, &format!("{target:?}"));
                if let Some(d) = data {
                    self.w.s(", data: ");
                    self.data_value(d);
                }
            }
            Transition::Update { owner, data, .. } => {
                self.w.s("update: { ");
                self.w.v(owner.span);
                self.w.s(": ");
                if owner.name == "root" {
                    self.context_params(&[data], &["root"]);
                    self.w.s("({ ...root, ");
                } else {
                    self.context_params(&[data], &["ancestors"]);
                    self.w.s(&format!("({{ ...ancestors.{}, ", owner.name));
                }
                self.object_entries(data);
                self.w.s(" }) }");
            }
        }
        if let Some(g) = guard {
            self.w.s(", guard: ");
            self.context_params(&[g], &[]);
            self.expr(g);
        }
        self.w.s(" }");
        self.w.pop_node();
    }

    /// The entries of an object literal, without braces.
    fn object_entries(&mut self, data: &Expr) {
        let ExprKind::Object(props) = &data.kind else {
            self.w.s("...");
            self.expr(data);
            return;
        };
        self.w.push_node(data.span);
        for (i, p) in props.iter().enumerate() {
            if i > 0 {
                self.w.s(", ");
            }
            self.object_prop(p);
        }
        self.w.pop_node();
    }

    fn data_value(&mut self, d: &Expr) {
        if Self::uses_context(d) {
            self.context_params(&[d], &[]);
            self.arrow_body_expr(d);
        } else {
            self.expr(d);
        }
    }

    fn handlers(
        &mut self,
        ctx: &mut Ctx,
        path: &str,
        parallel: bool,
        root_fields: &[SchemaField],
        body: &StateBody,
        is_root: bool,
    ) {
        let mut entries = 0;
        let open = |g: &mut Self, entries: &mut i32| {
            if *entries > 0 {
                g.w.s(",");
            }
            *entries += 1;
            g.w.nl();
        };
        self.w.s("{");
        self.w.indent();
        if is_root && root_fields.iter().any(|f| f.default.is_some()) {
            open(self, &mut entries);
            let defaults: Vec<&Expr> = root_fields.iter().filter_map(|f| f.default.as_ref()).collect();
            self.w.s("root: ");
            self.context_params(&defaults, &[]);
            self.w.s("({ ");
            let mut first = true;
            for f in root_fields {
                if let Some(d) = &f.default {
                    if !first {
                        self.w.s(", ");
                    }
                    first = false;
                    self.w.v(f.name.span);
                    self.w.s(": ");
                    self.expr(d);
                }
            }
            self.w.s(" })");
        }
        let initials: Vec<&StateDecl> = body.states.iter().filter(|s| s.is_initial).collect();
        for extra in initials.iter().skip(1) {
            self.error(extra.name.span, format!("`{}` is not the only initial state here", extra.name.name));
        }
        for s in &body.states {
            let is_initial = s.is_initial || (initials.is_empty() && std::ptr::eq(s, &body.states[0]));
            if parallel && s.is_initial {
                self.error(s.name.span, "a parallel state enters every region; remove `initial`");
            }
            for f in &s.fields {
                if !is_initial && f.default.is_some() {
                    self.error(f.span, "field defaults are the starting data of the initial state; this state is entered by transitions, which give its data");
                }
            }
        }
        if !parallel && !body.states.is_empty() {
            open(self, &mut entries);
            self.w.s("initial: ");
            let first = initials.first().copied().unwrap_or(&body.states[0]);
            let target = if path.is_empty() { first.name.name.clone() } else { format!("{path}.{}", first.name.name) };
            self.w.push_node(first.name.span);
            self.w.s(&format!("{{ target: {target:?}"));
            if !first.fields.is_empty() {
                for f in &first.fields {
                    if f.default.is_none() && !f.optional {
                        self.error(
                            f.span,
                            format!(
                                "`{}` needs a default value: `{}` is the initial state",
                                f.name.name, first.name.name
                            ),
                        );
                    }
                }
                let defaults: Vec<&Expr> = first.fields.iter().filter_map(|f| f.default.as_ref()).collect();
                self.w.s(", data: ");
                if defaults.iter().any(|d| Self::uses_context(d)) {
                    self.context_params(&defaults, &[]);
                    self.w.s("(");
                }
                self.w.s("{ ");
                let mut n = 0;
                for f in &first.fields {
                    if let Some(d) = &f.default {
                        if n > 0 {
                            self.w.s(", ");
                        }
                        n += 1;
                        self.w.v(f.name.span);
                        self.w.s(": ");
                        self.expr(d);
                    }
                }
                self.w.s(" }");
                if defaults.iter().any(|d| Self::uses_context(d)) {
                    self.w.s(")");
                }
            }
            self.w.s(" }");
            self.w.pop_node();
        }
        if !body.ons.is_empty() {
            open(self, &mut entries);
            self.w.s("on: {");
            self.w.indent();
            let mut events: Vec<&str> = Vec::new();
            for on in &body.ons {
                if !events.contains(&on.event.name.as_str()) {
                    events.push(&on.event.name);
                }
            }
            for (i, ev) in events.iter().enumerate() {
                if i > 0 {
                    self.w.s(",");
                }
                self.w.nl();
                let handlers: Vec<&On> = body.ons.iter().filter(|o| o.event.name == *ev).collect();
                self.w.v(handlers[0].event.span);
                self.w.s(": ");
                if handlers.len() > 1 {
                    self.w.s("[");
                }
                for (j, on) in handlers.iter().enumerate() {
                    if j > 0 {
                        self.w.s(", ");
                    }
                    self.w.push_node(on.span);
                    self.transition(ctx, &on.transition, on.guard.as_ref(), path);
                    self.w.pop_node();
                }
                if handlers.len() > 1 {
                    self.w.s("]");
                }
            }
            self.w.dedent();
            self.w.nl();
            self.w.s("}");
        }
        if !body.invokes.is_empty() {
            open(self, &mut entries);
            self.w.s("invoke: ");
            let many = body.invokes.len() > 1;
            if many {
                self.w.s("[");
                self.w.indent();
            }
            for (i, inv) in body.invokes.iter().enumerate() {
                if many {
                    if i > 0 {
                        self.w.s(",");
                    }
                    self.w.nl();
                }
                self.invoke(ctx, inv, path, i);
            }
            if many {
                self.w.dedent();
                self.w.nl();
                self.w.s("]");
            }
        }
        if !body.states.is_empty() {
            open(self, &mut entries);
            self.w.s("states: {");
            self.w.indent();
            for (i, s) in body.states.iter().enumerate() {
                if i > 0 {
                    self.w.s(",");
                }
                self.w.nl();
                self.w.push_node(s.span);
                self.w.v(s.name.span);
                self.w.s(": ");
                let child = if path.is_empty() { s.name.name.clone() } else { format!("{path}.{}", s.name.name) };
                let empty = s.body.states.is_empty() && s.body.ons.is_empty() && s.body.invokes.is_empty();
                if empty {
                    self.w.s("{}");
                } else {
                    self.handlers(ctx, &child, s.parallel, &[], &s.body, false);
                }
                self.w.pop_node();
            }
            self.w.dedent();
            self.w.nl();
            self.w.s("}");
        }
        self.w.dedent();
        if entries > 0 {
            self.w.nl();
        }
        self.w.s("}");
    }

    fn invoke(&mut self, ctx: &mut Ctx, inv: &Invoke, path: &str, index: usize) {
        self.w.push_node(inv.span);
        self.w.s("{ src: ");
        match &inv.src {
            InvokeSrc::Named(id) => self.w.s_at(id.span, &format!("{:?}", id.name)),
            InvokeSrc::After(delay) => {
                let state = path.rsplit('.').next().filter(|s| !s.is_empty()).unwrap_or("root");
                let name = format!("{}After{}", lower_first(state), index + 1);
                ctx.timers.push((name.clone(), delay.clone()));
                self.w.s_at(delay.span, &format!("{name:?}"));
            }
        }
        if let Some(input) = &inv.input {
            self.w.s(", input: ");
            self.context_params(&[input], &[]);
            self.arrow_body_expr(input);
        }
        for (name, tr) in &inv.handlers {
            let key = match name.name.as_str() {
                "done" => "onDone",
                "failure" => "onFailure",
                "element" => "onElement",
                "snapshot" => "onSnapshot",
                other => {
                    let span: Span = name.span;
                    self.error(
                        span,
                        format!("unknown invocation outcome `{other}`; expected done, failure, element or snapshot"),
                    );
                    "onDone"
                }
            };
            self.w.s(&format!(", {key}: "));
            self.transition(ctx, tr, None, path);
        }
        self.w.s(" }");
        self.w.pop_node();
    }
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
