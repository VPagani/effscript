use std::collections::{HashMap, HashSet};

use efs_syntax::ast::*;
use efs_syntax::{Diagnostic, Span};

use crate::writer::{Segment, Writer};
use crate::{Options, Output, Target};

/// Effect modules whose namespace exports a type of the same name, so that a bare
/// `Effect<A, E, R>` in type position can mean `Effect.Effect<A, E, R>`.
const TYPE_NAMESPACES: &[&str] = &[
    "Effect", "Stream", "Layer", "Option", "Exit", "Cause", "Chunk", "Fiber", "Queue", "Ref", "Scope", "Schedule",
    "Duration", "HashMap", "HashSet", "Deferred", "PubSub", "Result", "Redacted", "DateTime", "Config", "Brand",
    "Context", "Request", "Metric", "Channel", "Sink", "Schema", "Latch", "Pool",
];

/// The prelude: core modules of `effect` that every EffScript file can use without importing
/// them. A file that uses one gets `import { Name } from "effect"` (merged into an existing
/// import of `"effect"`), unless the file declares or imports that name itself or a local
/// binding shadows it where it is used. Names that would shadow JavaScript globals (`Array`,
/// `String`, `Number`, `Function`, `Record`, ...) are left out on purpose.
///
/// The list follows the supported Effect major (Effect 4, see docs/compatibility.md): every
/// name is a namespace export of the `effect` package's index.
pub const PRELUDE: &[&str] = &[
    "Effect", "Layer", "Stream", "Schema", "Console", "Schedule", "Duration", "Config", "Option", "Result", "Exit",
    "Cause", "Scope", "Fiber", "Ref", "Queue", "Clock", "Random", "DateTime", "Match",
];

fn runtime_module(name: &str) -> &'static str {
    match name {
        "Machine" => "@typeonce/effect-machine",
        "Html" => "@effect/ui/html",
        "UiSchema" => "@effect/ui",
        _ => "effect",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Binding {
    Import { module: String, name: String },
    Local,
}

pub(crate) struct Gen<'s> {
    pub(crate) src: &'s str,
    opts: &'s Options,
    pub(crate) w: Writer<'s>,
    pub(crate) diags: Vec<Diagnostic>,
    bindings: HashMap<String, Binding>,
    /// Runtime imports to add: (imported name, local name, module).
    needed: Vec<(String, String, &'static str)>,
    pub(crate) in_gen: bool,
    /// Prefix for the span names of `effect` methods, such as `UserRepo`.
    pub(crate) fn_prefix: Option<String>,
    /// Identifier rewrites in scope (component messages).
    pub(crate) renames: Vec<HashMap<String, String>>,
    /// The name of the markup builder in scope.
    pub(crate) builder: String,
    /// The component being lowered, for bare message names in event handlers.
    pub(crate) comp: Option<crate::component::CompCtx>,
    /// Inside a component update block.
    pub(crate) update: Option<crate::component::UpdateCtx>,
    /// For each module imported with braces, where extra names can be inserted.
    import_slots: Vec<(String, u32)>,
    /// Errors declared with `error` in this module; their instances can be yielded directly.
    local_errors: Vec<String>,
    /// JavaScript output: the imported names used as values (other imports are removed).
    pub(crate) keep_imports: Option<HashSet<String>>,
    /// Runtime names that resolved to the user's own imports.
    rt_hits: HashSet<String>,
    /// `run` and `use` sites, and what local names are.
    pub(crate) binds: crate::bind::Binds,
    /// The prelude names bound by the scopes around the code being lowered, innermost last.
    scopes: Vec<Vec<String>>,
    /// Source spans of the names that refer to the prelude.
    prelude_refs: Vec<Span>,
}

impl<'s> Gen<'s> {
    pub(crate) fn new(src: &'s str, opts: &'s Options) -> Gen<'s> {
        Gen {
            src,
            opts,
            w: Writer::new(src),
            diags: Vec::new(),
            bindings: HashMap::new(),
            needed: Vec::new(),
            in_gen: false,
            fn_prefix: None,
            renames: Vec::new(),
            builder: "h".into(),
            comp: None,
            update: None,
            import_slots: Vec::new(),
            local_errors: Vec::new(),
            keep_imports: None,
            rt_hits: HashSet::new(),
            binds: Default::default(),
            scopes: Vec::new(),
            prelude_refs: Vec::new(),
        }
    }

    pub(crate) fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(span, msg).with_code("codegen"));
    }

    /// The local name to use for a runtime binding such as `Effect` or `Machine`.
    pub(crate) fn rt(&mut self, name: &str) -> String {
        let module = runtime_module(name);
        let local = match self.bindings.get(name) {
            Some(Binding::Import { module: m, name: n }) if m == module && n == name => {
                self.rt_hits.insert(name.to_string());
                return name.to_string();
            }
            None => name.to_string(),
            Some(_) => format!("{name}$"),
        };
        if !self.needed.iter().any(|(n, _, _)| n == name) {
            self.needed.push((name.to_string(), local.clone(), module));
        }
        local
    }

    /// Writes `Runtime.member`.
    pub(crate) fn rtm(&mut self, ns: &str, member: &str) {
        let ns = self.rt(ns);
        self.w.s(&format!("{ns}.{member}"));
    }

    /// Enters a scope that binds `names` (only prelude names matter).
    pub(crate) fn push_scope(&mut self, names: Vec<String>) {
        self.scopes.push(names.into_iter().filter(|n| PRELUDE.contains(&n.as_str())).collect());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Whether `name`, referenced here, means the prelude module of that name: the prelude is
    /// on, and neither the file nor an enclosing scope binds the name.
    fn is_prelude(&self, name: &str) -> bool {
        self.opts.prelude
            && PRELUDE.contains(&name)
            && !self.bindings.contains_key(name)
            && !self.scopes.iter().any(|s| s.iter().any(|n| n == name))
    }

    /// Notes a reference to `name` at `span`. A prelude name gets its import, as long as the
    /// reference is written out (types in JavaScript output are not).
    pub(crate) fn reference(&mut self, name: &str, span: Span) {
        if self.w.mute == 0 && self.is_prelude(name) {
            self.rt(name);
            self.prelude_refs.push(span);
        }
    }

    fn collect_bindings(&mut self, module: &Module) {
        for item in &module.items {
            match &item.kind {
                ItemKind::Import(imp) => {
                    let m = imp.source.value().to_string();
                    if let Some(d) = &imp.default {
                        self.bindings
                            .insert(d.name.clone(), Binding::Import { module: m.clone(), name: "default".into() });
                    }
                    if let Some(n) = &imp.namespace {
                        self.bindings.insert(n.name.clone(), Binding::Import { module: m.clone(), name: "*".into() });
                    }
                    for s in &imp.named {
                        let local = s.alias.as_ref().unwrap_or(&s.name);
                        self.bindings.insert(
                            local.name.clone(),
                            Binding::Import { module: m.clone(), name: s.name.name.clone() },
                        );
                    }
                }
                ItemKind::Let(l) => self.bind_pattern(&l.pattern),
                ItemKind::Fn(f) => self.local(&f.name),
                ItemKind::Type(t) => self.local(&t.name),
                ItemKind::Service(s) => self.local(&s.name),
                ItemKind::Layer(l) => self.local(&l.name),
                ItemKind::Error(e) => {
                    self.local(&e.name);
                    self.local_errors.push(e.name.name.clone());
                }
                ItemKind::Schema(s) => self.local(&s.name),
                ItemKind::Enum(e) => self.local(&e.name),
                ItemKind::Machine(m) => {
                    self.local(&m.name);
                    for suffix in ["State", "Events"] {
                        self.bindings.insert(format!("{}{suffix}", m.name.name), Binding::Local);
                    }
                }
                ItemKind::Component(c) => {
                    self.local(&c.name);
                    for suffix in ["Message", "Events", "Props", "Model"] {
                        self.bindings.insert(format!("{}{suffix}", c.name.name), Binding::Local);
                    }
                }
                ItemKind::ExportList(_) | ItemKind::Stmt(_) => {}
            }
        }
    }

    fn local(&mut self, id: &Ident) {
        self.bindings.insert(id.name.clone(), Binding::Local);
    }

    fn bind_pattern(&mut self, p: &Pattern) {
        match &p.kind {
            PatternKind::Ident(id) => self.local(id),
            PatternKind::Object { props, rest } => {
                for prop in props {
                    match &prop.value {
                        Some(v) => self.bind_pattern(v),
                        None => self.local(&prop.key),
                    }
                }
                if let Some(r) = rest {
                    self.local(r);
                }
            }
            PatternKind::Array { elems, rest } => {
                for e in elems.iter().flatten() {
                    self.bind_pattern(e);
                }
                if let Some(r) = rest {
                    self.bind_pattern(r);
                }
            }
            PatternKind::Default { pattern, .. } => self.bind_pattern(pattern),
        }
    }

    /// Whether `name` refers to an Effect module namespace that has a same-named type: an
    /// import from `"effect"` or the prelude.
    fn is_type_namespace(&self, name: &str) -> bool {
        TYPE_NAMESPACES.contains(&name)
            && (self.is_prelude(name)
                || matches!(self.bindings.get(name), Some(Binding::Import { module, name: n }) if (module == "effect" && n == name) || (n == "*" && module == &format!("effect/{name}"))))
    }

    pub(crate) fn js(&self) -> bool {
        self.opts.target == Target::JavaScript
    }

    /// Writes type syntax, which JavaScript output leaves out.
    pub(crate) fn ts_only(&mut self, f: impl FnOnce(&mut Self)) {
        let js = self.js();
        if js {
            self.w.mute += 1;
        }
        f(self);
        if js {
            self.w.mute -= 1;
        }
    }

    fn generate(&mut self, module: &Module) {
        self.collect_bindings(module);
        self.binds.collect(module);
        let mut prev: Option<&Item> = None;
        for item in &module.items {
            let before = self.w.out.len();
            if let Some(prev) = prev {
                self.w.s("\n");
                let blank = self.src[prev.span.end as usize..item.span.start as usize].matches('\n').count() > 1;
                let both_imports = matches!(prev.kind, ItemKind::Import(_)) && matches!(item.kind, ItemKind::Import(_));
                if blank || !both_imports && !matches!(prev.kind, ItemKind::Stmt(_) | ItemKind::Let(_)) {
                    self.w.s("\n");
                }
            }
            let separated = self.w.out.len();
            self.item(item);
            if self.w.out.len() == separated {
                // Nothing was written (a type in JavaScript output): drop the separator too.
                self.w.out.truncate(before);
                continue;
            }
            prev = Some(item);
        }
        self.w.s("\n");
    }

    /// The names the generated code uses as values: identifiers in expressions, schema
    /// references written as types, components in markup and runtime names.
    pub(crate) fn used_values(mut self, module: &Module) -> HashSet<String> {
        self.generate(module);
        let mut used = std::mem::take(&mut self.rt_hits);
        collect_value_names(module, &mut used);
        used
    }

    pub(crate) fn module(mut self, module: &Module) -> Output {
        self.generate(module);

        // Runtime imports join an existing braced import of the same module, or get their own line.
        let mut modules: Vec<&'static str> = Vec::new();
        for (_, _, m) in &self.needed {
            if !modules.contains(m) {
                modules.push(m);
            }
        }
        let mut header = String::new();
        let mut inserts: Vec<(u32, String)> = Vec::new();
        for m in modules {
            let names: Vec<String> = self
                .needed
                .iter()
                .filter(|(_, _, mm)| *mm == m)
                .map(|(n, l, _)| if n == l { n.clone() } else { format!("{n} as {l}") })
                .collect();
            match self.import_slots.iter().find(|(module, _)| module == m) {
                Some(&(_, at)) => inserts.push((at, format!(", {}", names.join(", ")))),
                None => header.push_str(&format!("import {{ {} }} from \"{m}\"\n", names.join(", "))),
            }
        }
        let mut binds = self.resolve_binds();
        inserts.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        for (at, text) in inserts {
            let len = text.len() as u32;
            self.w.out.insert_str(at as usize, &text);
            for b in &mut binds {
                if b.gen_pos >= at {
                    b.gen_pos += len;
                }
            }
            for seg in &mut self.w.segs {
                if seg.gen_start >= at {
                    seg.gen_start += len;
                } else if seg.gen_start + seg.gen_len > at {
                    seg.gen_len += len;
                }
            }
        }
        if !header.is_empty() && !module.items.first().is_some_and(|i| matches!(i.kind, ItemKind::Import(_))) {
            header.push('\n');
        }
        let shift = header.len() as u32;
        let mut code = header;
        code.push_str(&self.w.out);
        let segments = self.w.segs.iter().map(|s| Segment { gen_start: s.gen_start + shift, ..*s }).collect();
        for b in &mut binds {
            b.gen_pos += shift;
        }
        Output { code, segments, diagnostics: self.diags, binds, prelude_refs: self.prelude_refs }
    }

    // -----------------------------------------------------------------------------------------
    // Items
    // -----------------------------------------------------------------------------------------

    fn export(&mut self, item: &Item) {
        if item.export {
            self.w.s("export ");
        }
    }

    fn item(&mut self, item: &Item) {
        self.w.push_node(item.span);
        match &item.kind {
            ItemKind::Import(imp) => self.import(imp),
            ItemKind::ExportList(list) => {
                let specs: Vec<ImportSpec> =
                    list.specs.iter().filter(|s| !(self.js() && s.type_only)).cloned().collect();
                if !list.star && specs.is_empty() && !list.specs.is_empty() {
                    self.w.pop_node();
                    return;
                }
                if list.source.is_none() {
                    for s in &specs {
                        self.reference(&s.name.name, s.name.span);
                    }
                }
                self.w.s("export ");
                if list.star {
                    self.w.s("*");
                } else {
                    self.specs(&specs);
                }
                if let Some(src) = &list.source {
                    self.w.s(" from ");
                    self.module_specifier(src);
                }
            }
            ItemKind::Let(l) => {
                self.export(item);
                self.let_decl(l);
            }
            ItemKind::Fn(f) => {
                self.export(item);
                self.fn_decl(f);
            }
            ItemKind::Type(t) => self.ts_only(|g| {
                g.export(item);
                g.w.s("type ");
                g.w.v(t.name.span);
                g.push_scope(t.type_params.iter().map(|tp| tp.name.name.clone()).collect());
                g.type_params(&t.type_params);
                g.w.s(" = ");
                g.ty(&t.ty);
                g.pop_scope();
            }),
            ItemKind::Service(s) => self.service(item, s),
            ItemKind::Layer(l) => self.layer(item, l),
            ItemKind::Error(e) => {
                self.export(item);
                self.w.s("class ");
                self.w.v(e.name.span);
                self.w.s(" extends ");
                self.rtm("Data", "TaggedError");
                self.w.s(&format!("(\"{}\")", e.name.name));
                self.ts_only(|g| {
                    g.w.s("<");
                    g.members(&e.fields, true);
                    g.w.s(">");
                });
                self.w.s(" {}");
            }
            ItemKind::Schema(s) => self.schema_decl(item, s),
            ItemKind::Enum(e) => self.enum_decl(item, e),
            ItemKind::Machine(m) => self.machine(item, m),
            ItemKind::Component(c) => self.component(item, c),
            ItemKind::Stmt(s) => self.stmt_inner(s),
        }
        self.w.pop_node();
    }

    fn specs(&mut self, specs: &[ImportSpec]) {
        self.w.s("{ ");
        for (i, s) in specs.iter().enumerate() {
            if i > 0 {
                self.w.s(", ");
            }
            if s.type_only {
                self.w.s("type ");
            }
            self.w.v(s.name.span);
            if let Some(a) = &s.alias {
                self.w.s(" as ");
                self.w.v(a.span);
            }
        }
        self.w.s(" }");
    }

    fn module_specifier(&mut self, s: &StrLit) {
        let value = s.value();
        if let Some(stem) = value.strip_suffix(".efs")
            && (value.starts_with('.') || value.starts_with('/'))
            && self.opts.import_extension != ".efs"
        {
            let quote = &s.raw[..1];
            let text = format!("{quote}{stem}{}{quote}", self.opts.import_extension);
            self.w.s_at(s.span, &text);
        } else {
            self.w.v(s.span);
        }
    }

    fn import(&mut self, imp: &Import) {
        let keep = |name: &Ident, keep: &Option<HashSet<String>>| keep.as_ref().is_none_or(|k| k.contains(&name.name));
        let (default, namespace, named) = if self.js() {
            if imp.type_only {
                return;
            }
            let default = imp.default.as_ref().filter(|d| keep(d, &self.keep_imports));
            let namespace = imp.namespace.as_ref().filter(|n| keep(n, &self.keep_imports));
            let named: Vec<ImportSpec> = imp
                .named
                .iter()
                .filter(|s| !s.type_only && keep(s.alias.as_ref().unwrap_or(&s.name), &self.keep_imports))
                .cloned()
                .collect();
            let side_effect_only = imp.default.is_none() && imp.namespace.is_none() && !imp.has_braces;
            if !side_effect_only && default.is_none() && namespace.is_none() && named.is_empty() {
                return;
            }
            (default, namespace, named)
        } else {
            (imp.default.as_ref(), imp.namespace.as_ref(), imp.named.clone())
        };
        self.w.s("import ");
        if imp.type_only {
            self.w.s("type ");
        }
        let mut any = false;
        if let Some(d) = default {
            self.w.v(d.span);
            any = true;
        }
        if let Some(n) = namespace {
            if any {
                self.w.s(", ");
            }
            self.w.s("* as ");
            self.w.v(n.span);
            any = true;
        }
        if imp.has_braces && (!named.is_empty() || !self.js()) {
            if any {
                self.w.s(", ");
            }
            self.specs(&named);
            if !imp.type_only && !named.is_empty() {
                // Just before the closing ` }`.
                self.import_slots.push((imp.source.value().to_string(), self.w.pos() - 2));
            }
            any = true;
        }
        if any {
            self.w.s(" from ");
        }
        self.module_specifier(&imp.source);
    }

    fn let_decl(&mut self, l: &LetDecl) {
        self.w.s(if l.kind == LetKind::Const { "const " } else { "let " });
        self.pattern(&l.pattern);
        if let Some(t) = &l.ty {
            self.ts_only(|g| {
                g.w.s(": ");
                g.ty(t);
            });
        }
        if let Some(init) = &l.init {
            self.w.s(" = ");
            self.expr_top(init);
        }
    }

    pub(crate) fn yield_star(&mut self, span: Span, kind: BindKind) {
        if self.update.is_some() && !self.in_gen {
            if kind == BindKind::Run {
                self.error(
                    span,
                    "in an update, `run` issues a command and must be a statement of its own at the top of the update, outside any `if` or loop",
                );
            }
        } else if !self.in_gen {
            self.error(
                span,
                format!(
                    "`{}` can only be used inside an `effect function`, an `effect` arrow or an `effect {{ }}` block",
                    kind.keyword()
                ),
            );
        }
        self.w.s("yield* ");
    }

    fn fn_decl(&mut self, f: &FnDecl) {
        self.bound(&f.name);
        if f.func.effect {
            self.w.s("const ");
            self.w.v(f.name.span);
            if let Some(ret) = &f.func.ret {
                self.ts_only(|g| {
                    g.w.s(": ");
                    g.type_params(&f.func.type_params);
                    g.params(&f.func.params);
                    g.w.s(" => ");
                    g.ty(ret);
                });
            }
            self.w.s(" = ");
            self.rtm("Effect", "fn");
            let name = match &self.fn_prefix {
                Some(p) => format!("{p}.{}", f.name.name),
                None => f.name.name.clone(),
            };
            self.w.s(&format!("({name:?})("));
            self.generator_fn(&f.func);
            self.w.s(")");
            return;
        }
        self.w.s("function ");
        self.w.v(f.name.span);
        self.push_scope(func_names(&f.func));
        self.type_params(&f.func.type_params);
        self.params(&f.func.params);
        if let Some(ret) = &f.func.ret {
            self.ts_only(|g| {
                g.w.s(": ");
                g.ty(ret);
            });
        }
        self.w.s(" ");
        self.func_body_block(&f.func, false);
        self.pop_scope();
    }

    /// `function* <T>(params) { body }`
    pub(crate) fn generator_fn(&mut self, func: &Func) {
        self.w.s("function* ");
        self.push_scope(func_names(func));
        self.type_params(&func.type_params);
        self.params(&func.params);
        self.w.s(" ");
        self.func_body_block(func, true);
        self.pop_scope();
    }

    fn func_body_block(&mut self, func: &Func, generator: bool) {
        let saved = std::mem::replace(&mut self.in_gen, generator);
        let saved_update = self.update.take();
        match &func.body {
            FuncBody::Block(b) => self.block(b),
            FuncBody::Expr(e) => {
                self.w.s("{");
                self.w.indent();
                self.w.nl();
                self.w.s("return ");
                if let ExprKind::Bind(kind, inner) = &e.kind {
                    self.bind(e.span, *kind, inner);
                } else {
                    self.expr_top(e);
                }
                self.w.dedent();
                self.w.nl();
                self.w.s("}");
            }
        }
        self.in_gen = saved;
        self.update = saved_update;
    }

    fn service(&mut self, item: &Item, s: &Service) {
        self.export(item);
        self.w.s("class ");
        self.w.v(s.name.span);
        self.w.s(" extends ");
        self.rtm("Context", "Service");
        self.ts_only(|g| {
            g.w.s("<");
            g.w.v(s.name.span);
            g.w.s(", ");
            g.members(&s.members, true);
            g.w.s(">");
        });
        self.w.s("()(");
        match &s.key {
            Some(k) => self.w.v(k.span),
            None => self.w.s(&format!("\"{}\"", s.name.name)),
        }
        self.w.s(") {}");
    }

    fn layer(&mut self, item: &Item, l: &LayerDecl) {
        self.export(item);
        self.w.s("const ");
        self.w.v(l.name.span);
        self.w.s(" = ");
        let saved = self.fn_prefix.replace(l.service.span.text(self.src).to_string());
        match &l.body {
            LayerBody::Effect(b) => {
                self.rtm("Layer", "effect");
                self.w.s("(");
                self.expr(&l.service);
                self.w.s(", ");
                self.effect_gen(b);
                self.w.s(")");
            }
            LayerBody::Value(v) => {
                self.rtm("Layer", "succeed");
                self.w.s("(");
                self.expr(&l.service);
                self.w.s(", ");
                self.expr_top(v);
                self.w.s(")");
            }
        }
        self.fn_prefix = saved;
    }

    /// `Effect.gen(function* () { ... })`
    pub(crate) fn effect_gen(&mut self, b: &Block) {
        self.rtm("Effect", "gen");
        self.w.s("(function* () ");
        let saved = std::mem::replace(&mut self.in_gen, true);
        self.block(b);
        self.in_gen = saved;
        self.w.s(")");
    }

    fn enum_decl(&mut self, item: &Item, e: &EnumDecl) {
        let js = self.js();
        if js {
            self.w.mute += 1;
        }
        self.export(item);
        self.w.s("type ");
        self.w.v(e.name.span);
        self.w.s(" = ");
        self.rtm("Data", "TaggedEnum");
        self.w.s("<{");
        self.w.indent();
        for v in &e.variants {
            self.w.push_node(v.span);
            self.w.nl();
            self.w.v(v.name.span);
            self.w.s(": {");
            for (i, f) in v.fields.iter().enumerate() {
                self.w.push_node(f.span);
                self.w.s(if i == 0 { " " } else { "; " });
                self.w.s("readonly ");
                self.w.v(f.name.span);
                if f.optional {
                    self.w.s("?");
                }
                self.w.s(": ");
                match &f.value {
                    SchemaFieldValue::Type(t) => self.ty(t),
                    SchemaFieldValue::Expr(x) => {
                        self.error(x.span, "enum fields take types, not schema expressions");
                        self.w.s("unknown");
                    }
                }
                self.w.pop_node();
            }
            self.w.s(if v.fields.is_empty() { "}" } else { " }" });
            self.w.pop_node();
        }
        self.w.dedent();
        self.w.nl();
        self.w.s("}>");
        self.w.nl();
        if js {
            self.w.mute -= 1;
        }
        self.export(item);
        self.w.s("const ");
        self.w.v(e.name.span);
        self.w.s(" = ");
        self.rtm("Data", "taggedEnum");
        self.ts_only(|g| {
            g.w.s("<");
            g.w.v(e.name.span);
            g.w.s(">");
        });
        self.w.s("()");
    }

    // -----------------------------------------------------------------------------------------
    // Types
    // -----------------------------------------------------------------------------------------

    pub(crate) fn members(&mut self, members: &[TypeMember], readonly: bool) {
        if self.js() {
            return;
        }
        if members.is_empty() {
            self.w.s("{}");
            return;
        }
        self.w.s("{");
        self.w.indent();
        for m in members {
            self.w.push_node(m.span);
            self.w.nl();
            if let TypeMemberKind::Index { param, key, value } = &m.kind {
                if m.readonly || readonly {
                    self.w.s("readonly ");
                }
                self.w.s("[");
                self.w.v(param.span);
                self.w.s(": ");
                self.ty(key);
                self.w.s("]: ");
                self.ty(value);
            } else {
                if m.readonly || readonly {
                    self.w.s("readonly ");
                }
                self.prop_key(&m.name);
                if m.optional {
                    self.w.s("?");
                }
                self.w.s(": ");
                match &m.kind {
                    TypeMemberKind::Prop(t) => self.ty(t),
                    TypeMemberKind::Method { type_params, params, ret } => {
                        self.type_params(type_params);
                        self.params(params);
                        self.w.s(" => ");
                        match ret {
                            Some(r) => self.ty(r),
                            None => self.w.s("void"),
                        }
                    }
                    TypeMemberKind::Index { .. } => unreachable!(),
                }
            }
            self.w.pop_node();
        }
        self.w.dedent();
        self.w.nl();
        self.w.s("}");
    }

    pub(crate) fn type_params(&mut self, tps: &[TypeParam]) {
        if tps.is_empty() || self.js() {
            return;
        }
        self.w.s("<");
        for (i, tp) in tps.iter().enumerate() {
            if i > 0 {
                self.w.s(", ");
            }
            if tp.is_const {
                self.w.s("const ");
            }
            self.w.v(tp.name.span);
            if let Some(c) = &tp.constraint {
                self.w.s(" extends ");
                self.ty(c);
            }
            if let Some(d) = &tp.default {
                self.w.s(" = ");
                self.ty(d);
            }
        }
        self.w.s(">");
    }

    pub(crate) fn ty(&mut self, t: &Type) {
        if self.js() {
            return;
        }
        self.w.push_node(t.span);
        match &t.kind {
            TypeKind::Ref { name, args } => {
                if name.len() == 1 && self.is_type_namespace(&name[0].name) {
                    self.w.v(name[0].span);
                    self.w.s(".");
                }
                self.reference(&name[0].name, name[0].span);
                for (i, n) in name.iter().enumerate() {
                    if i > 0 {
                        self.w.s(".");
                    }
                    self.w.v(n.span);
                }
                if let Some(args) = args {
                    self.w.s("<");
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            self.w.s(", ");
                        }
                        self.ty(a);
                    }
                    self.w.s(">");
                }
            }
            TypeKind::Lit(_) | TypeKind::Template(_) => self.w.v(t.span),
            TypeKind::Array(inner) => {
                let needs_parens =
                    matches!(inner.kind, TypeKind::Union(_) | TypeKind::Intersection(_) | TypeKind::Fn { .. });
                if needs_parens {
                    self.w.s("(");
                }
                self.ty(inner);
                if needs_parens {
                    self.w.s(")");
                }
                self.w.s("[]");
            }
            TypeKind::Tuple(elems) => {
                self.w.s("[");
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        self.w.s(", ");
                    }
                    if e.rest {
                        self.w.s("...");
                    }
                    if let Some(n) = &e.name {
                        self.w.v(n.span);
                        if e.optional {
                            self.w.s("?");
                        }
                        self.w.s(": ");
                        self.ty(&e.ty);
                    } else {
                        self.ty(&e.ty);
                        if e.optional {
                            self.w.s("?");
                        }
                    }
                }
                self.w.s("]");
            }
            TypeKind::Object(members) => self.members(members, false),
            TypeKind::Union(ms) | TypeKind::Intersection(ms) => {
                let sep = if matches!(t.kind, TypeKind::Union(_)) { " | " } else { " & " };
                for (i, m) in ms.iter().enumerate() {
                    if i > 0 {
                        self.w.s(sep);
                    }
                    if matches!(m.kind, TypeKind::Fn { .. }) {
                        self.w.s("(");
                        self.ty(m);
                        self.w.s(")");
                    } else {
                        self.ty(m);
                    }
                }
            }
            TypeKind::Fn { type_params, params, ret } => {
                self.type_params(type_params);
                self.params(params);
                self.w.s(" => ");
                self.ty(ret);
            }
            TypeKind::Paren(inner) => {
                self.w.s("(");
                self.ty(inner);
                self.w.s(")");
            }
            TypeKind::Typeof(name) => {
                self.reference(&name[0].name, name[0].span);
                self.w.s("typeof ");
                for (i, n) in name.iter().enumerate() {
                    if i > 0 {
                        self.w.s(".");
                    }
                    self.w.v(n.span);
                }
            }
            TypeKind::Keyof(inner) => {
                self.w.s("keyof ");
                self.ty(inner);
            }
            TypeKind::Readonly(inner) => {
                self.w.s("readonly ");
                self.ty(inner);
            }
            TypeKind::Indexed { object, index } => {
                self.ty(object);
                self.w.s("[");
                self.ty(index);
                self.w.s("]");
            }
        }
        self.w.pop_node();
    }

    pub(crate) fn params(&mut self, params: &[Param]) {
        self.w.s("(");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.w.s(", ");
            }
            self.w.push_node(p.span);
            if p.rest {
                self.w.s("...");
            }
            self.pattern(&p.pattern);
            self.ts_only(|g| {
                if p.optional {
                    g.w.s("?");
                }
                if let Some(t) = &p.ty {
                    g.w.s(": ");
                    g.ty(t);
                }
            });
            if let Some(d) = &p.default {
                self.w.s(" = ");
                self.expr(d);
            }
            self.w.pop_node();
        }
        self.w.s(")");
    }

    pub(crate) fn pattern(&mut self, p: &Pattern) {
        self.w.push_node(p.span);
        match &p.kind {
            PatternKind::Ident(id) => {
                self.bound(id);
                self.w.v(id.span)
            }
            PatternKind::Object { props, rest } => {
                self.w.s("{ ");
                for (i, prop) in props.iter().enumerate() {
                    if i > 0 {
                        self.w.s(", ");
                    }
                    self.w.v(prop.key.span);
                    if let Some(v) = &prop.value {
                        self.w.s(": ");
                        self.pattern(v);
                    } else {
                        self.bound(&prop.key);
                    }
                    if let Some(d) = &prop.default {
                        self.w.s(" = ");
                        self.expr(d);
                    }
                }
                if let Some(r) = rest {
                    if !props.is_empty() {
                        self.w.s(", ");
                    }
                    self.w.s("...");
                    self.bound(r);
                    self.w.v(r.span);
                }
                self.w.s(" }");
            }
            PatternKind::Array { elems, rest } => {
                self.w.s("[");
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        self.w.s(", ");
                    }
                    if let Some(e) = e {
                        self.pattern(e);
                    }
                }
                if let Some(r) = rest {
                    if !elems.is_empty() {
                        self.w.s(", ");
                    }
                    self.w.s("...");
                    self.pattern(r);
                }
                self.w.s("]");
            }
            PatternKind::Default { pattern, default } => {
                self.pattern(pattern);
                self.w.s(" = ");
                self.expr(default);
            }
        }
        self.w.pop_node();
    }

    pub(crate) fn prop_key(&mut self, k: &PropKey) {
        match k {
            PropKey::Ident(i) => self.w.v(i.span),
            PropKey::Str(s) => self.w.v(s.span),
            PropKey::Num(_, span) => self.w.v(*span),
            PropKey::Computed(e) => {
                self.w.s("[");
                self.expr(e);
                self.w.s("]");
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------------------------

    pub(crate) fn block(&mut self, b: &Block) {
        self.push_scope(block_decls(b));
        self.w.push_node(b.span);
        self.w.s("{");
        self.w.indent();
        for s in &b.stmts {
            self.w.nl();
            self.stmt(s);
        }
        self.w.dedent();
        self.w.nl();
        self.w.s("}");
        self.w.pop_node();
        self.pop_scope();
    }

    pub(crate) fn stmt_in_update(&mut self, s: &Stmt) {
        self.stmt_inner(s);
    }

    fn stmt(&mut self, s: &Stmt) {
        self.w.push_node(s.span);
        self.stmt_inner(s);
        self.w.pop_node();
    }

    fn stmt_inner(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let(l) => self.let_decl(l),
            StmtKind::Fn(f) => self.fn_decl(f),
            StmtKind::Expr(e) => {
                if let ExprKind::Bind(kind, inner) = &e.kind {
                    self.bind(e.span, *kind, inner);
                } else {
                    // An object literal or function expression would otherwise start a block or declaration.
                    let wrap = matches!(e.kind, ExprKind::Object(_));
                    if wrap {
                        self.w.s("(");
                    }
                    self.expr_top(e);
                    if wrap {
                        self.w.s(")");
                    }
                }
            }
            StmtKind::Emit(e) => {
                if self.update.is_none() {
                    self.error(s.span, "`emit` can only be used in a component `update`");
                }
                if let Some(u) = &mut self.update {
                    u.has_emit = true;
                }
                self.w.s("$events.push(");
                self.expr_top(e);
                self.w.s(")");
            }
            StmtKind::Return(e) => {
                if self.update.is_some() {
                    self.error(s.span, "an update block ends by itself; use `if`/`else` instead of `return`");
                }
                self.w.s("return");
                if let Some(e) = e {
                    self.w.s(" ");
                    if let ExprKind::Bind(kind, inner) = &e.kind {
                        self.bind(e.span, *kind, inner);
                    } else {
                        self.expr_top(e);
                    }
                }
            }
            StmtKind::If(i) => self.if_stmt(i),
            StmtKind::For { kind, pattern, iter, body, bind } => {
                if *bind {
                    self.yield_star(s.span, BindKind::Run);
                    self.rtm("Stream", "runForEach");
                    self.w.s("(");
                    self.expr(iter);
                    self.w.s(", (");
                    self.push_scope(pattern_names(pattern));
                    self.pattern(pattern);
                    self.w.s(") => ");
                    // A body that only runs one effect needs no generator.
                    match body.stmts.as_slice() {
                        [
                            Stmt {
                                kind: StmtKind::Expr(Expr { kind: ExprKind::Bind(BindKind::Run, inner), .. }), ..
                            },
                        ] => self.arrow_body_expr(inner),
                        _ => self.effect_gen(body),
                    }
                    self.pop_scope();
                    self.w.s(")");
                } else {
                    self.w.s(if *kind == LetKind::Let { "for (let " } else { "for (const " });
                    self.pattern(pattern);
                    self.w.s(" of ");
                    self.expr(iter);
                    self.w.s(") ");
                    self.push_scope(pattern_names(pattern));
                    self.block(body);
                    self.pop_scope();
                }
            }
            StmtKind::While { cond, body } => {
                self.w.s("while (");
                self.expr(cond);
                self.w.s(") ");
                self.block(body);
            }
            StmtKind::Break => self.w.s("break"),
            StmtKind::Continue => self.w.s("continue"),
            StmtKind::Fail(e) => {
                if !self.in_gen {
                    self.error(s.span, "`fail` can only be used inside an effect body");
                }
                self.w.s("return yield* ");
                // Tagged errors are yieldable: `yield* new NotFound(...)` fails with them directly.
                let yieldable = matches!(&e.kind, ExprKind::New { callee, .. }
                    if matches!(&callee.kind, ExprKind::Ident(n) if self.local_errors.contains(n)));
                if yieldable {
                    self.expr_top(e);
                } else {
                    self.rtm("Effect", "fail");
                    self.w.s("(");
                    self.expr_top(e);
                    self.w.s(")");
                }
            }
            StmtKind::Defer(e) => {
                if !self.in_gen {
                    self.error(s.span, "`defer` can only be used inside an effect body");
                }
                self.w.s("yield* ");
                self.rtm("Effect", "addFinalizer");
                self.w.s("(() => ");
                self.expr_top(e);
                self.w.s(")");
            }
            StmtKind::Throw(e) => {
                self.w.s("throw ");
                self.expr_top(e);
            }
            StmtKind::Block(b) => self.block(b),
        }
    }

    fn if_stmt(&mut self, i: &IfStmt) {
        self.w.s("if (");
        self.expr_top(&i.cond);
        self.w.s(") ");
        self.block(&i.then);
        if let Some(e) = &i.else_ {
            self.w.s(" else ");
            match &e.kind {
                StmtKind::If(inner) => {
                    self.w.push_node(e.span);
                    self.if_stmt(inner);
                    self.w.pop_node();
                }
                StmtKind::Block(b) => self.block(b),
                _ => self.stmt(e),
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------------------------

    /// An expression in a position where `yield*` needs no parentheses.
    pub(crate) fn expr_top(&mut self, e: &Expr) {
        if let ExprKind::Bind(kind, inner) = &e.kind {
            self.w.push_node(e.span);
            self.bind(e.span, *kind, inner);
            self.w.pop_node();
        } else {
            self.expr(e);
        }
    }

    /// An expression used as the body of a synthesized arrow function.
    pub(crate) fn arrow_body_expr(&mut self, e: &Expr) {
        if matches!(e.kind, ExprKind::Object(_)) {
            self.w.s("(");
            self.expr(e);
            self.w.s(")");
        } else {
            self.expr(e);
        }
    }

    fn lookup_rename(&self, name: &str) -> Option<String> {
        self.renames.iter().rev().find_map(|m| m.get(name).cloned())
    }

    pub(crate) fn expr(&mut self, e: &Expr) {
        self.w.push_node(e.span);
        self.expr_inner(e);
        self.w.pop_node();
    }

    fn args(&mut self, args: &[ArrayElem]) {
        self.w.s("(");
        self.elems(args);
        self.w.s(")");
    }

    fn elems(&mut self, elems: &[ArrayElem]) {
        for (i, a) in elems.iter().enumerate() {
            if i > 0 {
                self.w.s(", ");
            }
            match a {
                ArrayElem::Expr(x) => self.expr(x),
                ArrayElem::Spread(x) => {
                    self.w.s("...");
                    self.expr(x);
                }
                ArrayElem::Hole => {}
            }
        }
        if matches!(elems.last(), Some(ArrayElem::Hole)) {
            self.w.s(",");
        }
    }

    fn type_args(&mut self, args: &Option<Vec<Type>>) {
        if self.js() {
            return;
        }
        if let Some(args) = args {
            self.w.s("<");
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    self.w.s(", ");
                }
                self.ty(a);
            }
            self.w.s(">");
        }
    }

    fn expr_inner(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Ident(name) => {
                if let Some(q) = self.lookup_rename(name) {
                    self.w.s(&q);
                    self.w.s(".");
                } else {
                    self.reference(name, e.span);
                }
                self.w.v(e.span);
            }
            ExprKind::Lit(_) => self.w.v(e.span),
            ExprKind::Template(parts) => {
                for p in parts {
                    match p {
                        TemplatePart::Str(span) => self.w.v(*span),
                        TemplatePart::Expr(x) => self.expr_top(x),
                    }
                }
            }
            ExprKind::Array(elems) => {
                self.w.s("[");
                self.elems(elems);
                self.w.s("]");
            }
            ExprKind::Object(props) => self.object(props),
            ExprKind::Paren(inner) => {
                self.w.s("(");
                self.expr_top(inner);
                self.w.s(")");
            }
            ExprKind::Unary { op, expr } => {
                self.w.s(op);
                if op.chars().all(char::is_alphabetic) {
                    self.w.s(" ");
                }
                self.expr(expr);
            }
            ExprKind::Update { op, prefix, expr } => {
                if *prefix {
                    self.w.s(op);
                }
                self.expr(expr);
                if !*prefix {
                    self.w.s(op);
                }
            }
            ExprKind::Binary { op, left, right } => {
                self.expr(left);
                let op = match op.as_str() {
                    "==" => "===",
                    "!=" => "!==",
                    o => o,
                };
                self.w.s(&format!(" {op} "));
                self.expr(right);
            }
            ExprKind::Assign { op, target, value } => {
                self.expr(target);
                self.w.s(&format!(" {op} "));
                self.expr_top(value);
            }
            ExprKind::Cond { cond, then, else_ } => {
                self.expr(cond);
                self.w.s(" ? ");
                self.expr_top(then);
                self.w.s(" : ");
                self.expr_top(else_);
            }
            ExprKind::Call { callee, type_args, args, optional } => {
                self.expr(callee);
                if *optional {
                    self.w.s("?.");
                }
                self.type_args(type_args);
                self.args(args);
            }
            ExprKind::Member { object, name, optional } => {
                self.expr(object);
                self.w.s(if *optional { "?." } else { "." });
                self.w.v(name.span);
            }
            ExprKind::Index { object, index, optional } => {
                self.expr(object);
                self.w.s(if *optional { "?.[" } else { "[" });
                self.expr_top(index);
                self.w.s("]");
            }
            ExprKind::New { callee, type_args, args } => {
                self.w.s("new ");
                self.expr(callee);
                self.type_args(type_args);
                self.args(args);
            }
            ExprKind::Func(func) => self.func_expr(func, None),
            ExprKind::Pipe(parts) => {
                let pipe = self.rt("pipe");
                self.w.s(&pipe);
                self.w.s("(");
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        self.w.s(", ");
                    }
                    self.expr(p);
                }
                self.w.s(")");
            }
            ExprKind::With { expr, layer } => {
                self.rtm("Effect", "provide");
                self.w.s("(");
                self.expr_top(expr);
                self.w.s(", ");
                self.expr_top(layer);
                self.w.s(")");
            }
            ExprKind::Bind(kind, inner) => {
                self.w.s("(");
                self.bind(e.span, *kind, inner);
                self.w.s(")");
            }
            ExprKind::Catch { expr, handler } => self.catch(expr, handler),
            ExprKind::Match { expr, arms } => self.match_expr(expr, arms),
            ExprKind::If { cond, then, else_ } => {
                self.w.s("(");
                self.expr(cond);
                self.w.s(" ? ");
                self.expr_top(then);
                self.w.s(" : ");
                self.expr_top(else_);
                self.w.s(")");
            }
            ExprKind::EffectBlock(b) => self.effect_gen(b),
            ExprKind::Markup(el) => self.element(el),
            ExprKind::As { expr, ty } => {
                self.expr(expr);
                self.ts_only(|g| {
                    g.w.s(" as ");
                    g.ty(ty);
                });
            }
            ExprKind::Satisfies { expr, ty } => {
                self.expr(expr);
                self.ts_only(|g| {
                    g.w.s(" satisfies ");
                    g.ty(ty);
                });
            }
            ExprKind::NonNull(inner) => {
                self.expr(inner);
                self.ts_only(|g| g.w.s("!"));
            }
            ExprKind::Error => self.w.s("undefined as never"),
        }
    }

    /// A function expression. `name` gives an `effect` function its span name.
    fn func_expr(&mut self, func: &Func, name: Option<&str>) {
        if func.effect {
            match name {
                Some(n) => {
                    self.rtm("Effect", "fn");
                    self.w.s(&format!("({n:?})("));
                }
                None => {
                    self.rtm("Effect", "fnUntraced");
                    self.w.s("(");
                }
            }
            self.generator_fn(func);
            self.w.s(")");
            return;
        }
        self.push_scope(func_names(func));
        self.type_params(&func.type_params);
        self.params(&func.params);
        if let Some(r) = &func.ret {
            self.ts_only(|g| {
                g.w.s(": ");
                g.ty(r);
            });
        }
        self.w.s(" => ");
        let saved_update = self.update.take();
        let saved = std::mem::replace(&mut self.in_gen, false);
        match &func.body {
            FuncBody::Block(b) => self.block(b),
            FuncBody::Expr(x) => self.arrow_body_expr(x),
        }
        self.in_gen = saved;
        self.update = saved_update;
        self.pop_scope();
    }

    fn object(&mut self, props: &[Prop]) {
        if props.is_empty() {
            self.w.s("{}");
            return;
        }
        let multiline = props.iter().any(|p| matches!(p, Prop::Method { .. }))
            || props.iter().any(|p| matches!(p, Prop::KeyValue { value, .. } if matches!(value.kind, ExprKind::Func(_) | ExprKind::EffectBlock(_))));
        self.w.s("{");
        if multiline {
            self.w.indent();
        }
        for (i, p) in props.iter().enumerate() {
            if i > 0 {
                self.w.s(",");
            }
            if multiline {
                self.w.nl();
            } else {
                self.w.s(" ");
            }
            self.object_prop(p);
        }
        if multiline {
            self.w.dedent();
            self.w.nl();
        } else {
            self.w.s(" ");
        }
        self.w.s("}");
    }

    pub(crate) fn object_prop(&mut self, p: &Prop) {
        match p {
            Prop::KeyValue { key, value } => {
                self.prop_key(key);
                self.w.s(": ");
                self.expr_top(value);
            }
            Prop::Shorthand(id) => {
                if let Some(q) = self.lookup_rename(&id.name) {
                    self.w.v(id.span);
                    self.w.s(&format!(": {q}."));
                } else {
                    self.reference(&id.name, id.span);
                }
                self.w.v(id.span);
            }
            Prop::Spread(x) => {
                self.w.s("...");
                self.expr(x);
            }
            Prop::Method { key, func } => {
                self.w.push_node(func.span);
                if func.effect {
                    self.prop_key(key);
                    self.w.s(": ");
                    let key_name = match key {
                        PropKey::Ident(i) => i.name.clone(),
                        PropKey::Str(s) => s.value().to_string(),
                        _ => "method".to_string(),
                    };
                    let name = match &self.fn_prefix {
                        Some(p) => format!("{p}.{key_name}"),
                        None => key_name,
                    };
                    self.func_expr(func, Some(&name));
                } else {
                    self.prop_key(key);
                    self.push_scope(func_names(func));
                    self.type_params(&func.type_params);
                    self.params(&func.params);
                    if let Some(r) = &func.ret {
                        self.ts_only(|g| {
                            g.w.s(": ");
                            g.ty(r);
                        });
                    }
                    self.w.s(" ");
                    self.func_body_block(func, false);
                    self.pop_scope();
                }
                self.w.pop_node();
            }
        }
    }

    fn catch(&mut self, expr: &Expr, handler: &CatchHandler) {
        match handler {
            CatchHandler::Tags(arms) => {
                self.rtm("Effect", "catchTags");
                self.w.s("(");
                self.expr_top(expr);
                self.w.s(", {");
                self.w.indent();
                for (i, arm) in arms.iter().enumerate() {
                    self.w.push_node(arm.span);
                    if i > 0 {
                        self.w.s(",");
                    }
                    self.w.nl();
                    self.w.v(arm.tag.span);
                    self.w.s(": (");
                    self.push_scope(arm.binding.as_ref().map(pattern_names).unwrap_or_default());
                    if let Some(b) = &arm.binding {
                        self.pattern(b);
                    }
                    self.w.s(") => ");
                    self.arrow_body_expr(&arm.body);
                    self.pop_scope();
                    self.w.pop_node();
                }
                self.w.dedent();
                self.w.nl();
                self.w.s("})");
            }
            CatchHandler::All { binding, body } => {
                self.rtm("Effect", "catch");
                self.w.s("(");
                self.expr_top(expr);
                self.w.s(", (");
                self.push_scope(pattern_names(binding));
                self.pattern(binding);
                self.w.s(") => ");
                self.arrow_body_expr(body);
                self.pop_scope();
                self.w.s(")");
            }
        }
    }

    fn match_expr(&mut self, scrutinee: &Expr, arms: &[MatchArm]) {
        let all_tags = arms.iter().all(|a| matches!(a.pattern, MatchPattern::Tag { .. }));
        if all_tags {
            self.rtm("Match", "valueTags");
            self.w.s("(");
            self.expr_top(scrutinee);
            self.w.s(", {");
            self.w.indent();
            for (i, arm) in arms.iter().enumerate() {
                let MatchPattern::Tag { tag, binding } = &arm.pattern else { unreachable!() };
                self.w.push_node(arm.span);
                if i > 0 {
                    self.w.s(",");
                }
                self.w.nl();
                self.w.v(tag.span);
                self.w.s(": (");
                self.push_scope(binding.as_ref().map(pattern_names).unwrap_or_default());
                if let Some(b) = binding {
                    self.pattern(b);
                }
                self.w.s(") => ");
                self.arrow_body_expr(&arm.body);
                self.pop_scope();
                self.w.pop_node();
            }
            self.w.dedent();
            self.w.nl();
            self.w.s("})");
            return;
        }
        let m = self.rt("Match");
        self.w.s(&format!("{m}.value("));
        self.expr_top(scrutinee);
        self.w.s(").pipe(");
        self.w.indent();
        let mut has_default = false;
        for (i, arm) in arms.iter().enumerate() {
            self.w.push_node(arm.span);
            if i > 0 {
                self.w.s(",");
            }
            self.w.nl();
            let names = match &arm.pattern {
                MatchPattern::Tag { binding: Some(b), .. } => pattern_names(b),
                _ => Vec::new(),
            };
            self.push_scope(names);
            match &arm.pattern {
                MatchPattern::Tag { tag, binding } => {
                    self.w.s(&format!("{m}.tag("));
                    self.w.s_at(tag.span, &format!("{:?}", tag.name));
                    self.w.s(", (");
                    if let Some(b) = binding {
                        self.pattern(b);
                    }
                    self.w.s(") => ");
                }
                MatchPattern::Lit(_, span) => {
                    self.w.s(&format!("{m}.when("));
                    self.w.v(*span);
                    self.w.s(", () => ");
                }
                MatchPattern::Wildcard(span) => {
                    if i + 1 != arms.len() {
                        self.error(*span, "`_` must be the last arm");
                    }
                    has_default = true;
                    self.w.s(&format!("{m}.orElse(() => "));
                }
            }
            self.arrow_body_expr(&arm.body);
            self.pop_scope();
            self.w.s(")");
            self.w.pop_node();
        }
        if !has_default {
            self.w.s(",");
            self.w.nl();
            self.w.s(&format!("{m}.exhaustive"));
        }
        self.w.dedent();
        self.w.nl();
        self.w.s(")");
    }

    // -----------------------------------------------------------------------------------------
    // Markup
    // -----------------------------------------------------------------------------------------

    pub(crate) fn element(&mut self, el: &Element) {
        self.w.push_node(el.span);
        if el.is_component() {
            self.component_element(el);
            self.w.pop_node();
            return;
        }
        let builder = self.builder.clone();
        self.w.s(&format!("{builder}."));
        self.w.v(el.name[0].span);
        for a in &el.attrs {
            match a {
                Attr::Spread(x) => {
                    self.w.s(".attrs(");
                    self.expr(x);
                    self.w.s(")");
                }
                Attr::Named { name, value, span } => {
                    self.w.push_node(*span);
                    self.attr(name, value.as_ref());
                    self.w.pop_node();
                }
            }
        }
        let texts = normalized_texts(&el.children);
        let children: Vec<(usize, &Child)> = el
            .children
            .iter()
            .enumerate()
            .filter(|(i, c)| !matches!(c, Child::Text(..)) || texts[*i].is_some())
            .collect();
        if !children.is_empty() {
            let multiline = el.children.iter().any(|c| matches!(c, Child::Element(_)));
            self.w.s(".children(");
            if multiline {
                self.w.indent();
            }
            for (n, (i, c)) in children.iter().enumerate() {
                if n > 0 {
                    self.w.s(",");
                    if !multiline {
                        self.w.s(" ");
                    }
                }
                if multiline {
                    self.w.nl();
                }
                match c {
                    Child::Text(_, span) => {
                        let t = texts[*i].as_ref().unwrap();
                        self.w.s_at(*span, &format!("{t:?}"));
                    }
                    Child::Expr(x) => self.expr(x),
                    Child::Element(e) => self.element(e),
                }
            }
            if multiline {
                self.w.dedent();
                self.w.nl();
            }
            self.w.s(")");
        }
        self.w.pop_node();
    }

    fn attr(&mut self, name: &Ident, value: Option<&AttrValue>) {
        let n = name.name.as_str();
        let write_value = |g: &mut Self| match value {
            Some(AttrValue::Str(s)) => g.w.v(s.span),
            Some(AttrValue::Expr(x)) => g.expr_top(x),
            None => g.w.s("true"),
        };
        if let Some(event) = n.strip_prefix("on:") {
            let mut parts = event.split('|');
            let event = parts.next().unwrap_or_default();
            let modifiers: Vec<&str> = parts.collect();
            self.w.s(&format!(".on({event:?}, "));
            match value {
                Some(AttrValue::Expr(x)) => self.handler(x),
                _ => {
                    self.error(name.span, "event handlers need a value: `on:click={Message()}`");
                    self.w.s("undefined as never");
                }
            }
            if !modifiers.is_empty() {
                let opts: Vec<String> = modifiers.iter().map(|m| format!("{m}: true")).collect();
                self.w.s(&format!(", {{ {} }}", opts.join(", ")));
            }
            self.w.s(")");
            return;
        }
        match n {
            "class" => {
                self.w.s(".class(");
                match value {
                    Some(AttrValue::Expr(Expr { kind: ExprKind::Array(elems), .. })) => self.elems(elems),
                    _ => write_value(self),
                }
                self.w.s(")");
            }
            "style" | "value" | "checked" | "selected" | "disabled" | "modal" => {
                self.w.s(&format!(".{n}("));
                write_value(self);
                self.w.s(")");
            }
            "attrs" => {
                self.w.s(".attrs(");
                write_value(self);
                self.w.s(")");
            }
            _ => {
                self.w.s(&format!(".attr({n:?}, "));
                match value {
                    None => self.w.s("\"\""),
                    Some(_) => write_value(self),
                }
                self.w.s(")");
            }
        }
    }

    fn component_element(&mut self, el: &Element) {
        for (i, n) in el.name.iter().enumerate() {
            if i > 0 {
                self.w.s(".");
            }
            self.w.v(n.span);
        }
        let builder = self.builder.clone();
        self.w.s(&format!("({builder}"));
        let mut key = None;
        let mut on_emit = None;
        let mut on_events: Vec<(&Ident, &Expr)> = Vec::new();
        let mut props = Vec::new();
        for a in &el.attrs {
            match a {
                Attr::Named { name, value, .. } if name.name == "key" => key = Some((name, value)),
                Attr::Named { name, value, .. } if name.name == "onEmit" => on_emit = Some((name, value)),
                Attr::Named { name, value: Some(AttrValue::Expr(x)), .. } if name.name.starts_with("on:") => {
                    on_events.push((name, x))
                }
                Attr::Named { name, .. } if name.name.starts_with("on:") => {
                    self.error(name.span, "event handlers need a value: `on:Saved={SavedNote}`");
                }
                other => props.push(other),
            }
        }
        if on_emit.is_some() && !on_events.is_empty() {
            self.error(on_events[0].0.span, "use either `onEmit` or `on:Event` handlers, not both");
        }
        if !el.children.is_empty() {
            self.error(el.span, "components do not take children; pass them as props");
        }
        if key.is_none() && on_emit.is_none() && on_events.is_empty() && props.is_empty() {
            self.w.s(")");
            return;
        }
        self.w.s(", { ");
        let mut first = true;
        let mut sep = |g: &mut Self| {
            if !first {
                g.w.s(", ");
            }
            first = false;
        };
        for (label, entry) in [("key", key), ("onEmit", on_emit)] {
            if let Some((_, value)) = entry {
                sep(self);
                self.w.s(&format!("{label}: "));
                self.attr_value(value.as_ref());
            }
        }
        if !on_events.is_empty() {
            sep(self);
            // Matches the child's event by tag; returning `undefined` ignores the others.
            self.w.s("onEmit: (event) => ");
            for (name, handler) in &on_events {
                let tag = name.name.trim_start_matches("on:");
                self.w.s_at(name.span, &format!("event._tag === {tag:?} ? "));
                if matches!(handler.kind, ExprKind::Func(_)) {
                    self.w.s("(");
                    self.expr(handler);
                    self.w.s(")(event)");
                } else {
                    self.handler(handler);
                }
                self.w.s(" : ");
            }
            self.w.s("undefined");
        }
        if !props.is_empty() {
            sep(self);
            self.w.s("props: { ");
            for (i, p) in props.iter().enumerate() {
                if i > 0 {
                    self.w.s(", ");
                }
                match p {
                    Attr::Named { name, value, .. } => {
                        if name.name.contains(['-', ':', '|']) {
                            self.w.s_at(name.span, &format!("{:?}", name.name));
                        } else {
                            self.w.v(name.span);
                        }
                        self.w.s(": ");
                        self.attr_value(value.as_ref());
                    }
                    Attr::Spread(x) => {
                        self.w.s("...");
                        self.expr(x);
                    }
                }
            }
            self.w.s(" }");
        }
        self.w.s(" })");
    }

    /// An event handler: a message, a function of the payload, or the bare name of a
    /// message without fields (`on:click={ClickedReset}` means `ClickedReset()`).
    fn handler(&mut self, x: &Expr) {
        if let ExprKind::Ident(n) = &x.kind
            && let Some(comp) = &self.comp
            && let Some((_, has_fields)) = comp.messages.iter().find(|(m, _)| m == n)
        {
            if *has_fields {
                let n = n.clone();
                self.error(x.span, format!("`{n}` has fields; pass them: `{n}({{ ... }})`"));
            }
            self.expr(x);
            self.w.s("()");
            return;
        }
        self.expr_top(x);
    }

    fn attr_value(&mut self, value: Option<&AttrValue>) {
        match value {
            Some(AttrValue::Str(s)) => self.w.v(s.span),
            Some(AttrValue::Expr(x)) => self.expr_top(x),
            None => self.w.s("true"),
        }
    }
}

/// JSX-style whitespace handling: lines are trimmed, blank lines dropped and the rest
/// joined with single spaces. Inline whitespace next to expressions is kept.
fn normalized_texts(children: &[Child]) -> Vec<Option<String>> {
    children
        .iter()
        .map(|c| {
            let Child::Text(raw, _) = c else { return None };
            if !raw.contains('\n') {
                return Some(raw.clone());
            }
            let lines: Vec<&str> = raw.split('\n').collect();
            let mut parts = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                let mut l = *line;
                if i > 0 {
                    l = l.trim_start();
                }
                if i + 1 < lines.len() {
                    l = l.trim_end();
                }
                if !l.is_empty() {
                    parts.push(l);
                }
            }
            let s = parts.join(" ");
            if s.trim().is_empty() { None } else { Some(s) }
        })
        .collect()
}

/// Names a module uses as values. Schema fields are written as types but name schema
/// values, so their type references count too.
pub(crate) fn collect_value_names(module: &Module, out: &mut HashSet<String>) {
    use efs_syntax::visit::{walk_block, walk_expr};
    fn expr(e: &Expr, out: &mut HashSet<String>) {
        walk_expr(e, &mut |x| match &x.kind {
            ExprKind::Ident(n) => {
                out.insert(n.clone());
            }
            ExprKind::Markup(el) => element(el, out),
            _ => {}
        });
    }
    fn element(el: &Element, out: &mut HashSet<String>) {
        if el.is_component() {
            out.insert(el.name[0].name.clone());
        }
        for c in &el.children {
            if let Child::Element(e) = c {
                element(e, out);
            }
        }
    }
    fn block(b: &Block, out: &mut HashSet<String>) {
        walk_block(b, &mut |x| match &x.kind {
            ExprKind::Ident(n) => {
                out.insert(n.clone());
            }
            ExprKind::Markup(el) => element(el, out),
            _ => {}
        });
    }
    fn schema_type(t: &Type, out: &mut HashSet<String>) {
        match &t.kind {
            TypeKind::Ref { name, args } => {
                out.insert(name[0].name.clone());
                for a in args.iter().flatten() {
                    schema_type(a, out);
                }
            }
            TypeKind::Array(x) | TypeKind::Readonly(x) | TypeKind::Paren(x) => schema_type(x, out),
            TypeKind::Union(ms) | TypeKind::Intersection(ms) => ms.iter().for_each(|m| schema_type(m, out)),
            TypeKind::Tuple(es) => es.iter().for_each(|e| schema_type(&e.ty, out)),
            TypeKind::Object(ms) => {
                for m in ms {
                    if let TypeMemberKind::Prop(t) = &m.kind {
                        schema_type(t, out);
                    }
                }
            }
            _ => {}
        }
    }
    fn fields(fs: &[SchemaField], out: &mut HashSet<String>) {
        for f in fs {
            match &f.value {
                SchemaFieldValue::Type(t) => schema_type(t, out),
                SchemaFieldValue::Expr(e) => expr(e, out),
            }
            if let Some(d) = &f.default {
                expr(d, out);
            }
        }
    }
    fn func(f: &Func, out: &mut HashSet<String>) {
        for p in &f.params {
            if let Some(d) = &p.default {
                expr(d, out);
            }
        }
        match &f.body {
            FuncBody::Expr(e) => expr(e, out),
            FuncBody::Block(b) => block(b, out),
        }
    }
    fn state_body(b: &StateBody, out: &mut HashSet<String>) {
        let tr = |t: &Transition, out: &mut HashSet<String>| match t {
            Transition::Target { data: Some(d), .. } | Transition::Update { data: d, .. } => expr(d, out),
            _ => {}
        };
        for on in &b.ons {
            if let Some(g) = &on.guard {
                expr(g, out);
            }
            tr(&on.transition, out);
        }
        for inv in &b.invokes {
            if let Some(i) = &inv.input {
                expr(i, out);
            }
            if let InvokeSrc::After(d) = &inv.src {
                expr(d, out);
            }
            for (_, t) in &inv.handlers {
                tr(t, out);
            }
        }
        for s in &b.states {
            fields(&s.fields, out);
            state_body(&s.body, out);
        }
    }
    for item in &module.items {
        match &item.kind {
            ItemKind::Let(l) => {
                if let Some(i) = &l.init {
                    expr(i, out);
                }
            }
            ItemKind::Fn(f) => func(&f.func, out),
            ItemKind::Layer(l) => {
                expr(&l.service, out);
                match &l.body {
                    LayerBody::Effect(b) => block(b, out),
                    LayerBody::Value(v) => expr(v, out),
                }
            }
            ItemKind::Schema(s) => match &s.kind {
                SchemaKind::Struct(f) | SchemaKind::Class(f) | SchemaKind::Tagged(f) => fields(f, out),
                SchemaKind::Alias(t) => schema_type(t, out),
            },
            ItemKind::Machine(m) => {
                if let Some(t) = &m.input {
                    schema_type(t, out);
                }
                fields(&m.root_fields, out);
                for v in &m.events {
                    fields(&v.fields, out);
                }
                for s in &m.sources {
                    expr(&s.value, out);
                }
                state_body(&m.body, out);
            }
            ItemKind::Component(c) => {
                for m in &c.members {
                    match &m.kind {
                        MemberKind::Prop(f) | MemberKind::Let(f) => fields(std::slice::from_ref(f), out),
                        MemberKind::Event(v) => fields(&v.fields, out),
                        MemberKind::Update(u) => {
                            fields(&u.fields, out);
                            match &u.body {
                                UpdateBody::Expr(e) => expr(e, out),
                                UpdateBody::Block(b) => block(b, out),
                            }
                        }
                        MemberKind::View(v) | MemberKind::Option(_, v) => expr(v, out),
                    }
                }
            }
            ItemKind::Stmt(s) => block(&Block { stmts: vec![s.clone()], span: s.span }, out),
            ItemKind::ExportList(list) => {
                for s in &list.specs {
                    out.insert(s.name.name.clone());
                }
            }
            ItemKind::Import(_) | ItemKind::Type(_) | ItemKind::Service(_) | ItemKind::Error(_) | ItemKind::Enum(_) => {
            }
        }
        // An exported import is a value use.
    }
}

/// The names a pattern binds.
pub(crate) fn pattern_names(p: &Pattern) -> Vec<String> {
    fn go(p: &Pattern, out: &mut Vec<String>) {
        match &p.kind {
            PatternKind::Ident(id) => out.push(id.name.clone()),
            PatternKind::Object { props, rest } => {
                for prop in props {
                    match &prop.value {
                        Some(v) => go(v, out),
                        None => out.push(prop.key.name.clone()),
                    }
                }
                out.extend(rest.iter().map(|r| r.name.clone()));
            }
            PatternKind::Array { elems, rest } => {
                elems.iter().flatten().for_each(|e| go(e, out));
                if let Some(r) = rest {
                    go(r, out);
                }
            }
            PatternKind::Default { pattern, .. } => go(pattern, out),
        }
    }
    let mut out = Vec::new();
    go(p, &mut out);
    out
}

/// The names a function's parameters and type parameters bind.
fn func_names(f: &Func) -> Vec<String> {
    let mut out: Vec<String> = f.type_params.iter().map(|tp| tp.name.name.clone()).collect();
    for p in &f.params {
        out.extend(pattern_names(&p.pattern));
    }
    out
}

/// The names a block declares with `const`, `let` and `function`; they are in scope in the
/// whole block.
pub(crate) fn block_decls(b: &Block) -> Vec<String> {
    let mut out = Vec::new();
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Let(l) => out.extend(pattern_names(&l.pattern)),
            StmtKind::Fn(f) => out.push(f.name.name.clone()),
            _ => {}
        }
    }
    out
}
