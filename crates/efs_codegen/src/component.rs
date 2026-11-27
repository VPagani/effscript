//! Lowering `component` declarations to `@effect/ui` (`Html.component(...).make(...)`).
//!
//! ```efs
//! component Counter {
//!   prop label: String
//!   let count: Number = 0
//!   event Reset
//!
//!   update ClickedIncrement(amount: Number) {
//!     count += amount
//!   }
//!   view => <button on:click={ClickedIncrement({ amount: 1 })}>{label}: {String(count)}</button>
//! }
//! ```
//!
//! Props and model fields are used by their bare names: each function destructures the
//! ones it references. An update block works on local copies of the fields and returns
//! `$.evolve({ ...assigned })`, followed by `.run(...)` for commands and `.emit(...)` for
//! events.

use std::collections::HashMap;

use efs_syntax::Span;
use efs_syntax::ast::*;
use efs_syntax::visit::{walk_block, walk_expr};

use crate::lower::Gen;

/// The component being lowered.
#[derive(Clone, Debug)]
pub(crate) struct CompCtx {
    /// Message names and whether each has fields.
    pub(crate) messages: Vec<(String, bool)>,
    /// The name of the generated events enum.
    pub(crate) events_name: String,
}

/// State while lowering an update block.
#[derive(Clone, Debug, Default)]
pub(crate) struct UpdateCtx {
    pub(crate) has_emit: bool,
}

fn block_names(b: &Block) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    walk_block(b, &mut |e| {
        if let ExprKind::Ident(n) = &e.kind
            && !out.contains(n)
        {
            out.push(n.clone());
        }
    });
    out
}

fn expr_names(e: &Expr) -> Vec<String> {
    efs_syntax::visit::referenced_names(e)
}

fn target_names(t: &Expr, out: &mut Vec<(String, Span)>) {
    match &t.kind {
        ExprKind::Ident(n) => out.push((n.clone(), t.span)),
        ExprKind::Paren(x) => target_names(x, out),
        ExprKind::Array(elems) => {
            for e in elems {
                if let ArrayElem::Expr(x) | ArrayElem::Spread(x) = e {
                    target_names(x, out);
                }
            }
        }
        ExprKind::Object(props) => {
            for p in props {
                match p {
                    Prop::Shorthand(id) => out.push((id.name.clone(), id.span)),
                    Prop::KeyValue { value, .. } | Prop::Spread(value) => target_names(value, out),
                    Prop::Method { .. } => {}
                }
            }
        }
        _ => {}
    }
}

/// Calls its argument for every expression in some syntax.
type Visit<'a> = &'a mut dyn FnMut(&mut dyn FnMut(&Expr));

/// Names assigned anywhere in `visit`'s expressions (`x = ..`, `x += ..`, `x++`).
fn assigned_in(visit: Visit<'_>) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    visit(&mut |e| match &e.kind {
        ExprKind::Assign { target, .. } => target_names(target, &mut out),
        ExprKind::Update { expr, .. } => target_names(expr, &mut out),
        _ => {}
    });
    out
}

impl Gen<'_> {
    pub(crate) fn component(&mut self, item: &Item, c: &Component) {
        // Props and model fields are used by their bare names everywhere in the component.
        let names = c.props().chain(c.fields()).map(|f| f.name.name.clone()).collect();
        self.push_scope(names);
        self.component_inner(item, c);
        self.pop_scope();
    }

    fn component_inner(&mut self, item: &Item, c: &Component) {
        let export = if item.export { "export " } else { "" };
        let name = c.name.name.clone();
        let message_name = format!("{name}Message");
        let events_name = format!("{name}Events");
        let props: Vec<&SchemaField> = c.props().collect();
        let fields: Vec<&SchemaField> = c.fields().collect();
        let events: Vec<&Variant> = c.events().collect();
        let updates: Vec<&UpdateArm> = c.updates().collect();
        let prop_names: Vec<String> = props.iter().map(|p| p.name.name.clone()).collect();
        let field_names: Vec<String> = fields.iter().map(|f| f.name.name.clone()).collect();

        // Names must not be ambiguous.
        let mut seen: Vec<(String, &str)> = Vec::new();
        for (n, span, what) in props
            .iter()
            .map(|p| (&p.name.name, p.name.span, "prop"))
            .chain(fields.iter().map(|f| (&f.name.name, f.name.span, "model field")))
        {
            if let Some((_, other)) = seen.iter().find(|(s, _)| s == n) {
                self.error(span, format!("`{n}` is already declared as a {other}"));
            }
            seen.push((n.clone(), what));
        }
        let mut message_names: Vec<String> = Vec::new();
        for u in &updates {
            if message_names.contains(&u.message.name) {
                self.error(u.message.span, format!("`{}` already has an update", u.message.name));
            }
            if events.iter().any(|e| e.name.name == u.message.name) {
                self.error(u.message.span, format!("`{}` is both a message and an event; rename one", u.message.name));
            }
            for f in &u.fields {
                if prop_names.contains(&f.name.name) || field_names.contains(&f.name.name) {
                    self.error(
                        f.name.span,
                        format!("`{}` is also a prop or model field; give the message field another name", f.name.name),
                    );
                }
            }
            message_names.push(u.message.name.clone());
        }

        let mut unique: Vec<&UpdateArm> = Vec::new();
        for u in &updates {
            if !unique.iter().any(|x| x.message.name == u.message.name) {
                unique.push(u);
            }
        }
        if !unique.is_empty() {
            self.w.s(&format!("{export}const {message_name} = "));
            self.rtm("UiSchema", "TaggedEnum");
            self.w.s("({");
            self.w.indent();
            for (i, u) in unique.iter().enumerate() {
                self.w.push_node(u.span);
                if i > 0 {
                    self.w.s(",");
                }
                self.w.nl();
                self.w.v(u.message.span);
                self.w.s(": ");
                self.schema_struct_fields_inline(&u.fields);
                self.w.pop_node();
            }
            self.w.dedent();
            self.w.nl();
            self.w.s("})");
            self.w.nl();
        }
        if !events.is_empty() {
            self.w.s(&format!("{export}const {events_name} = "));
            self.rtm("UiSchema", "TaggedEnum");
            self.w.s("({");
            self.w.indent();
            for (i, v) in events.iter().enumerate() {
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
            self.w.s("})");
            self.w.nl();
        }

        self.w.s(&format!("{export}const "));
        self.w.v(c.name.span);
        self.w.s(" = ");
        self.rtm("Html", "component");
        self.w.s("({");
        self.w.indent();
        self.w.nl();
        self.w.s(&format!("name: {name:?},"));
        if !props.is_empty() {
            self.w.nl();
            self.w.s("props: ");
            self.rtm("Schema", "Struct");
            self.w.s("(");
            let owned: Vec<SchemaField> = props.iter().map(|p| SchemaField { default: None, ..(*p).clone() }).collect();
            self.schema_struct_fields_inline(&owned);
            self.w.s("),");
        }
        self.w.nl();
        self.w.s("model: ");
        self.rtm("Schema", "Struct");
        self.w.s("(");
        let owned: Vec<SchemaField> = fields.iter().map(|f| SchemaField { default: None, ..(*f).clone() }).collect();
        self.schema_struct_fields_inline(&owned);
        self.w.s("),");
        self.w.nl();
        self.w.s("message: ");
        if unique.is_empty() {
            self.rtm("Schema", "Never");
        } else {
            self.w.s(&message_name);
        }
        if !events.is_empty() {
            self.w.s(",");
            self.w.nl();
            self.w.s(&format!("events: {events_name}"));
        }
        self.w.dedent();
        self.w.nl();
        self.w.s("}).make({");
        self.w.indent();

        // Message and event constructors can be written bare inside the component.
        let mut renames = HashMap::new();
        for m in &message_names {
            renames.insert(m.clone(), message_name.clone());
        }
        for e in &events {
            renames.insert(e.name.name.clone(), events_name.clone());
        }
        self.renames.push(renames);
        let saved_comp = self.comp.replace(CompCtx {
            messages: unique.iter().map(|u| (u.message.name.clone(), !u.fields.is_empty())).collect(),
            events_name: events_name.clone(),
        });

        let has_init_option = c.options().any(|(k, _)| k.name == "init");
        let mut first_entry = true;
        let mut entry = |g: &mut Self| {
            if !first_entry {
                g.w.s(",");
            }
            first_entry = false;
            g.w.nl();
        };

        // init: the field defaults, which may read props.
        if !has_init_option {
            entry(self);
            self.w.s("init: ($) => ");
            let mut used_props: Vec<String> = Vec::new();
            for f in &fields {
                match &f.default {
                    Some(d) => {
                        for n in expr_names(d) {
                            if prop_names.contains(&n) && !used_props.contains(&n) {
                                used_props.push(n);
                            }
                        }
                    }
                    None => self.error(
                        f.span,
                        format!("give `{}` an initial value: `let {}: Type = value`", f.name.name, f.name.name),
                    ),
                }
            }
            let block = !used_props.is_empty();
            if block {
                self.w.s("{");
                self.w.indent();
                self.w.nl();
                self.w.s(&format!("const {{ {} }} = $.props", used_props.join(", ")));
                self.w.nl();
                self.w.s("return ");
            }
            self.w.s("({ ");
            let mut first = true;
            for f in &fields {
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
            if block {
                self.w.dedent();
                self.w.nl();
                self.w.s("}");
            }
        }

        // update
        if !updates.is_empty() {
            entry(self);
            self.w.s("update: {");
            self.w.indent();
            for (i, u) in updates.iter().enumerate() {
                if i > 0 {
                    self.w.s(",");
                }
                self.w.nl();
                self.update_arm(u, &prop_names, &field_names);
            }
            self.w.dedent();
            self.w.nl();
            self.w.s("}");
        } else if !c.options().any(|(k, _)| k.name == "update") {
            // Effect UI requires an `update`, even for a component without messages.
            entry(self);
            self.w.s("update: ($) => $.model");
        }

        // view
        match c.view() {
            Some(view) => {
                entry(self);
                for (n, span) in assigned_in(&mut |f| walk_expr(view, f)) {
                    if prop_names.contains(&n) || field_names.contains(&n) {
                        self.error(
                            span,
                            format!("the view cannot change `{n}`; send a message and assign it in an `update`"),
                        );
                    }
                }
                self.w.s("view: (h, $model) => ");
                let names = expr_names(view);
                let used_fields: Vec<&String> = field_names.iter().filter(|n| names.contains(n)).collect();
                let used_props: Vec<&String> = prop_names.iter().filter(|n| names.contains(n)).collect();
                let saved = std::mem::replace(&mut self.builder, "h".into());
                if used_fields.is_empty() && used_props.is_empty() {
                    self.arrow_body_expr(view);
                } else {
                    self.w.s("{");
                    self.w.indent();
                    if !used_fields.is_empty() {
                        self.w.nl();
                        self.w.s(&format!("const {{ {} }} = $model", join(&used_fields)));
                    }
                    if !used_props.is_empty() {
                        self.w.nl();
                        self.w.s(&format!("const {{ {} }} = h.props", join(&used_props)));
                    }
                    self.w.nl();
                    self.w.s("return ");
                    self.expr(view);
                    self.w.dedent();
                    self.w.nl();
                    self.w.s("}");
                }
                self.builder = saved;
            }
            None => self.error(c.name.span, "a component needs a `view`"),
        }

        for (key, value) in c.options() {
            entry(self);
            self.w.v(key.span);
            self.w.s(": ");
            self.expr_top(value);
        }

        self.comp = saved_comp;
        self.renames.pop();
        self.w.dedent();
        self.w.nl();
        self.w.s("})");
    }

    fn update_arm(&mut self, u: &UpdateArm, prop_names: &[String], field_names: &[String]) {
        self.push_scope(u.fields.iter().map(|f| f.name.name.clone()).collect());
        self.update_arm_inner(u, prop_names, field_names);
        self.pop_scope();
    }

    fn update_arm_inner(&mut self, u: &UpdateArm, prop_names: &[String], field_names: &[String]) {
        self.w.push_node(u.span);
        self.w.v(u.message.span);
        self.w.s(": ($");
        if !u.fields.is_empty() {
            let names: Vec<&str> = u.fields.iter().map(|f| f.name.name.as_str()).collect();
            self.w.s(&format!(", {{ {} }}", names.join(", ")));
        }
        self.w.s(") => ");
        match &u.body {
            UpdateBody::Expr(e) => {
                let names = expr_names(e);
                let used_props: Vec<&String> = prop_names.iter().filter(|n| names.contains(n)).collect();
                let used_fields: Vec<&String> = field_names.iter().filter(|n| names.contains(n)).collect();
                if used_props.is_empty() && used_fields.is_empty() {
                    self.arrow_body_expr(e);
                } else {
                    self.w.s("{");
                    self.w.indent();
                    if !used_props.is_empty() {
                        self.w.nl();
                        self.w.s(&format!("const {{ {} }} = $.props", join(&used_props)));
                    }
                    if !used_fields.is_empty() {
                        self.w.nl();
                        self.w.s(&format!("const {{ {} }} = $.model", join(&used_fields)));
                    }
                    self.w.nl();
                    self.w.s("return ");
                    self.expr(e);
                    self.w.dedent();
                    self.w.nl();
                    self.w.s("}");
                }
            }
            UpdateBody::Block(b) => self.update_block(b, prop_names, field_names),
        }
        self.w.pop_node();
    }

    fn update_block(&mut self, b: &Block, prop_names: &[String], field_names: &[String]) {
        let names = block_names(b);
        let assigned = assigned_in(&mut |f| walk_block(b, f));
        for (n, span) in &assigned {
            if prop_names.contains(n) {
                self.error(
                    *span,
                    format!("`{n}` is a prop and cannot be assigned; send an `event` to the parent instead"),
                );
            }
        }
        let mut assigned_fields: Vec<&String> = Vec::new();
        for f in field_names {
            if assigned.iter().any(|(n, _)| n == f) {
                assigned_fields.push(f);
            }
        }
        let read_fields: Vec<&String> =
            field_names.iter().filter(|n| names.contains(n) && !assigned_fields.contains(n)).collect();
        let used_props: Vec<&String> = prop_names.iter().filter(|n| names.contains(n)).collect();
        // Events emitted inside a branch are collected in an array; top-level ones are passed directly.
        let nested_emit = b.stmts.iter().any(|s| !matches!(s.kind, StmtKind::Emit(_)) && stmt_has_emit(s));
        self.push_scope(crate::lower::block_decls(b));

        self.w.push_node(b.span);
        self.w.s("{");
        self.w.indent();
        if !used_props.is_empty() {
            self.w.nl();
            self.w.s(&format!("const {{ {} }} = $.props", join(&used_props)));
        }
        if !read_fields.is_empty() {
            self.w.nl();
            self.w.s(&format!("const {{ {} }} = $.model", join(&read_fields)));
        }
        for f in &assigned_fields {
            self.w.nl();
            self.w.s(&format!("let {f} = $.model.{f}"));
        }
        if nested_emit {
            self.w.nl();
            let events = self.comp.as_ref().map(|c| c.events_name.clone()).unwrap_or_default();
            self.w.s("const $events");
            self.ts_only(|g| g.w.s(&format!(": Array<typeof {events}.Type>")));
            self.w.s(" = []");
        }
        let saved_update = self.update.replace(UpdateCtx::default());
        let mut commands: Vec<String> = Vec::new();
        let mut emitted: Vec<String> = Vec::new();
        for s in &b.stmts {
            self.w.nl();
            self.w.push_node(s.span);
            if let StmtKind::Expr(Expr { kind: ExprKind::Bind(BindKind::Run, inner), .. }) = &s.kind {
                let name = format!("$command{}", commands.len() + 1);
                self.w.s(&format!("const {name} = "));
                self.expr_top(inner);
                commands.push(name);
            } else if let (StmtKind::Emit(e), false) = (&s.kind, nested_emit) {
                let name = format!("$event{}", emitted.len() + 1);
                self.w.s(&format!("const {name} = "));
                self.expr_top(e);
                emitted.push(name);
            } else {
                self.stmt_in_update(s);
            }
            self.w.pop_node();
        }
        self.update = saved_update;
        self.w.nl();
        self.w.s("return ");
        let mut step = if assigned_fields.is_empty() {
            "$".to_string()
        } else {
            format!("$.evolve({{ {} }})", join(&assigned_fields))
        };
        if !commands.is_empty() {
            step.push_str(&format!(".run({})", commands.join(", ")));
        }
        if nested_emit {
            step.push_str(".emit(...$events)");
        } else if !emitted.is_empty() {
            step.push_str(&format!(".emit({})", emitted.join(", ")));
        }
        if step == "$" {
            step = "$.model".into();
        }
        self.w.s(&step);
        self.w.dedent();
        self.w.nl();
        self.w.s("}");
        self.w.pop_node();
        self.pop_scope();
    }
}

fn stmt_has_emit(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Emit(_) => true,
        StmtKind::If(i) => i.then.stmts.iter().any(stmt_has_emit) || i.else_.as_deref().is_some_and(stmt_has_emit),
        StmtKind::Block(b) | StmtKind::While { body: b, .. } | StmtKind::For { body: b, .. } => {
            b.stmts.iter().any(stmt_has_emit)
        }
        _ => false,
    }
}

fn join(names: &[&String]) -> String {
    names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
}
