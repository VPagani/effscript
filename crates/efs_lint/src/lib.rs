//! Lint rules for EffScript. Rules work on the syntax tree, so they run instantly and need no
//! type information; type errors come from the checker.
//!
//! A rule can be silenced for one line with a comment on the line before it:
//! `// efs-ignore: rule-name` (or `// efs-ignore` for every rule).

use efs_syntax::ast::*;
use efs_syntax::visit::{walk_block, walk_expr};
use efs_syntax::{Diagnostic, Severity, Span};

pub struct Rule {
    pub name: &'static str,
    pub severity: Severity,
    pub description: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        name: "discarded-effect",
        severity: Severity::Warning,
        description: "An effect created as a statement inside an effect body is never run; run it with `run`.",
    },
    Rule {
        name: "throw-in-effect",
        severity: Severity::Warning,
        description: "`throw` inside an effect body becomes a defect; use `fail` for typed errors.",
    },
    Rule {
        name: "prefer-fail",
        severity: Severity::Info,
        description: "`return run Effect.fail(e)` reads better as `fail e`.",
    },
    Rule {
        name: "prefer-const",
        severity: Severity::Warning,
        description: "A `let` binding that is never reassigned should be `const`.",
    },
    Rule { name: "unused-binding", severity: Severity::Warning, description: "A local binding is never used." },
    Rule {
        name: "pascal-case",
        severity: Severity::Warning,
        description: "Services, errors, schemas, enums, machines, components, states, events and messages use PascalCase.",
    },
    Rule {
        name: "unknown-event",
        severity: Severity::Error,
        description: "A machine handles an event it does not declare.",
    },
    Rule {
        name: "unreachable-state",
        severity: Severity::Warning,
        description: "A machine state that no transition enters.",
    },
    Rule {
        name: "message-naming",
        severity: Severity::Warning,
        description: "Effect UI messages are facts named verb-first in the past tense (`ClickedSave`), not commands (`SetName`).",
    },
    Rule {
        name: "strict-equality",
        severity: Severity::Info,
        description: "`==` already compiles to `===`; write `==`.",
    },
];

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Rules to skip.
    pub disabled: Vec<String>,
}

pub fn lint(src: &str, module: &Module, config: &Config) -> Vec<Diagnostic> {
    let mut l = Linter { diags: Vec::new() };
    for item in &module.items {
        l.item(item);
    }
    let ignores = ignore_comments(src);
    let index = efs_syntax::LineIndex::new(src);
    l.diags
        .into_iter()
        .filter(|d| {
            let rule = d.code.strip_prefix("lint/").unwrap_or(&d.code);
            if config.disabled.iter().any(|r| r == rule) {
                return false;
            }
            let (line, _) = index.position(d.span.start);
            !ignores.iter().any(|(l, r)| *l + 1 == line && r.as_ref().is_none_or(|r| r.iter().any(|x| x == rule)))
        })
        .collect()
}

/// Parses and lints source text.
pub fn lint_source(src: &str, config: &Config) -> Vec<Diagnostic> {
    let parsed = efs_syntax::parse(src);
    lint(src, &parsed.module, config)
}

fn ignore_comments(src: &str) -> Vec<(u32, Option<Vec<String>>)> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("// efs-ignore") {
            let rules = rest.trim_start_matches(':').trim();
            let rules = if rules.is_empty() {
                None
            } else {
                Some(rules.split([',', ' ']).filter(|s| !s.is_empty()).map(String::from).collect())
            };
            out.push((i as u32, rules));
        }
    }
    out
}

fn pattern_names(p: &Pattern, out: &mut Vec<(String, Span)>) {
    match &p.kind {
        PatternKind::Ident(id) => out.push((id.name.clone(), id.span)),
        PatternKind::Object { props, rest } => {
            for prop in props {
                match &prop.value {
                    Some(v) => pattern_names(v, out),
                    None => out.push((prop.key.name.clone(), prop.key.span)),
                }
            }
            if let Some(r) = rest {
                out.push((r.name.clone(), r.span));
            }
        }
        PatternKind::Array { elems, rest } => {
            for e in elems.iter().flatten() {
                pattern_names(e, out);
            }
            if let Some(r) = rest {
                pattern_names(r, out);
            }
        }
        PatternKind::Default { pattern, .. } => pattern_names(pattern, out),
    }
}

/// Names written by an assignment target: `x = ..`, `[a, b] = ..`, `{ a } = ..`.
fn assigned_names(target: &Expr, out: &mut Vec<String>) {
    match &target.kind {
        ExprKind::Ident(n) => out.push(n.clone()),
        ExprKind::Paren(x) => assigned_names(x, out),
        ExprKind::Array(elems) => {
            for e in elems {
                if let ArrayElem::Expr(x) | ArrayElem::Spread(x) = e {
                    assigned_names(x, out);
                }
            }
        }
        ExprKind::Object(props) => {
            for p in props {
                match p {
                    Prop::Shorthand(id) => out.push(id.name.clone()),
                    Prop::KeyValue { value, .. } | Prop::Spread(value) => assigned_names(value, out),
                    Prop::Method { .. } => {}
                }
            }
        }
        _ => {}
    }
}

fn severity_of(rule: &str) -> Severity {
    RULES.iter().find(|r| r.name == rule).map_or(Severity::Warning, |r| r.severity)
}

struct Linter {
    diags: Vec<Diagnostic>,
}

fn is_pascal(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase()) && !name.contains('_')
}

/// The leftmost identifier of a member chain: `Effect` in `Effect.log(x)`.
fn root_ident(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Ident(n) => Some(n),
        ExprKind::Member { object, .. } | ExprKind::Call { callee: object, .. } | ExprKind::Index { object, .. } => {
            root_ident(object)
        }
        _ => None,
    }
}

impl Linter {
    fn report(&mut self, rule: &str, span: Span, message: impl Into<String>) {
        self.diags.push(Diagnostic {
            severity: severity_of(rule),
            span,
            message: message.into(),
            code: format!("lint/{rule}"),
        });
    }

    fn pascal(&mut self, id: &Ident, what: &str) {
        if !is_pascal(&id.name) {
            self.report("pascal-case", id.span, format!("{what} `{}` should be PascalCase", id.name));
        }
    }

    fn item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Let(l) => {
                if let Some(e) = &l.init {
                    self.expr(e, false);
                }
            }
            ItemKind::Fn(f) => self.func(&f.func),
            ItemKind::Service(s) => self.pascal(&s.name, "service"),
            ItemKind::Error(e) => self.pascal(&e.name, "error"),
            ItemKind::Schema(s) => self.pascal(&s.name, "schema"),
            ItemKind::Enum(e) => {
                self.pascal(&e.name, "enum");
                for v in &e.variants {
                    self.pascal(&v.name, "variant");
                }
            }
            ItemKind::Layer(l) => match &l.body {
                LayerBody::Effect(b) => self.block(b, true),
                LayerBody::Value(v) => self.expr(v, false),
            },
            ItemKind::Machine(m) => self.machine(m),
            ItemKind::Component(c) => self.component(c),
            ItemKind::Stmt(s) => self.stmt(s, false),
            ItemKind::Import(_) | ItemKind::ExportList(_) | ItemKind::Type(_) => {}
        }
    }

    fn func(&mut self, f: &Func) {
        match &f.body {
            FuncBody::Block(b) => self.block(b, f.effect),
            FuncBody::Expr(e) => self.expr(e, f.effect),
        }
    }

    fn block(&mut self, b: &Block, in_effect: bool) {
        for (i, s) in b.stmts.iter().enumerate() {
            self.stmt(s, in_effect);
            if let StmtKind::Let(l) = &s.kind
                && let PatternKind::Ident(id) = &l.pattern.kind
                && !id.name.starts_with('_')
            {
                let rest = Block { stmts: b.stmts[i + 1..].to_vec(), span: b.span };
                let mut used = false;
                walk_block(&rest, &mut |e| {
                    if matches!(&e.kind, ExprKind::Ident(n) if *n == id.name) {
                        used = true;
                    }
                });
                if !used {
                    let hint = if l.init.as_ref().is_some_and(|i| matches!(i.kind, ExprKind::Bind(_))) {
                        format!(
                            "; to run the effect without keeping its result, write `run ...` or name it `_{}`",
                            id.name
                        )
                    } else {
                        String::new()
                    };
                    self.report("unused-binding", id.span, format!("`{}` is never used{hint}", id.name));
                }
            }
            if let StmtKind::Let(l) = &s.kind
                && l.kind == LetKind::Let
                && l.init.is_some()
            {
                let mut names = Vec::new();
                pattern_names(&l.pattern, &mut names);
                let rest = Block { stmts: b.stmts[i + 1..].to_vec(), span: b.span };
                let mut assigned: Vec<String> = Vec::new();
                walk_block(&rest, &mut |e| {
                    let target = match &e.kind {
                        ExprKind::Assign { target, .. } => Some(target),
                        ExprKind::Update { expr, .. } => Some(expr),
                        _ => None,
                    };
                    if let Some(t) = target {
                        assigned_names(t, &mut assigned);
                    }
                });
                if !names.is_empty() && names.iter().all(|(n, _)| !assigned.contains(n)) {
                    let span = names[0].1;
                    let what = if names.len() == 1 {
                        format!("`{}` is", names[0].0)
                    } else {
                        "these bindings are".to_string()
                    };
                    self.report("prefer-const", span, format!("{what} never reassigned; use `const`"));
                }
            }
        }
    }

    fn stmt(&mut self, s: &Stmt, in_effect: bool) {
        match &s.kind {
            StmtKind::Let(l) => {
                if let Some(e) = &l.init {
                    self.expr(e, in_effect);
                }
            }
            StmtKind::Fn(f) => self.func(&f.func),
            StmtKind::Expr(e) => {
                if in_effect {
                    let creates_effect = match &e.kind {
                        ExprKind::Pipe(_)
                        | ExprKind::With { .. }
                        | ExprKind::Catch { .. }
                        | ExprKind::EffectBlock(_) => true,
                        ExprKind::Call { callee, .. } => {
                            root_ident(callee).is_some_and(|r| r.starts_with(|c: char| c.is_ascii_uppercase()))
                        }
                        _ => false,
                    };
                    if creates_effect {
                        self.report(
                            "discarded-effect",
                            e.span,
                            "this effect is created but never run; did you mean `run ...`?",
                        );
                    }
                }
                self.expr(e, in_effect);
            }
            StmtKind::Return(Some(e)) => {
                if in_effect
                    && let ExprKind::Bind(inner) = &e.kind
                    && let ExprKind::Call { callee, .. } = &inner.kind
                    && let ExprKind::Member { object, name, .. } = &callee.kind
                    && matches!(&object.kind, ExprKind::Ident(n) if n == "Effect")
                    && name.name == "fail"
                {
                    self.report("prefer-fail", e.span, "write `fail error` instead of `return run Effect.fail(error)`");
                }
                self.expr(e, in_effect);
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::If(i) => self.if_stmt(i, in_effect),
            StmtKind::For { iter, body, bind, .. } => {
                self.expr(iter, in_effect);
                self.block(body, in_effect || *bind);
            }
            StmtKind::While { cond, body } => {
                self.expr(cond, in_effect);
                self.block(body, in_effect);
            }
            StmtKind::Fail(e) | StmtKind::Defer(e) | StmtKind::Emit(e) => self.expr(e, in_effect),
            StmtKind::Throw(e) => {
                if in_effect {
                    self.report(
                        "throw-in-effect",
                        s.span,
                        "`throw` inside an effect becomes a defect; use `fail` for an expected error",
                    );
                }
                self.expr(e, in_effect);
            }
            StmtKind::Block(b) => self.block(b, in_effect),
        }
    }

    fn if_stmt(&mut self, i: &IfStmt, in_effect: bool) {
        self.expr(&i.cond, in_effect);
        self.block(&i.then, in_effect);
        if let Some(e) = &i.else_ {
            self.stmt(e, in_effect);
        }
    }

    /// Visits nested functions and effect blocks, and expression-level checks.
    fn expr(&mut self, e: &Expr, _in_effect: bool) {
        let mut nested: Vec<(Block, bool)> = Vec::new();
        let mut funcs: Vec<Func> = Vec::new();
        let mut strict: Vec<Span> = Vec::new();
        walk_expr(e, &mut |x| match &x.kind {
            ExprKind::EffectBlock(b) => nested.push((b.clone(), true)),
            ExprKind::Func(f) => funcs.push((**f).clone()),
            ExprKind::Object(props) => {
                for p in props {
                    if let Prop::Method { func, .. } = p {
                        funcs.push(func.clone());
                    }
                }
            }
            ExprKind::Binary { op, left, .. } if op == "===" || op == "!==" => {
                strict.push(Span::new(left.span.end, left.span.end));
            }
            _ => {}
        });
        for s in strict {
            self.report("strict-equality", s, "`==` and `!=` already compile to `===` and `!==`");
        }
        // Only the outermost nested bodies: deeper ones are reached by recursion.
        let outer = |s: Span, all: &[Span]| !all.iter().any(|o| *o != s && o.start <= s.start && s.end <= o.end);
        let mut spans: Vec<Span> = nested.iter().map(|(b, _)| b.span).collect();
        spans.extend(funcs.iter().map(|f| f.span));
        for (b, eff) in &nested {
            if outer(b.span, &spans) {
                self.block(b, *eff);
            }
        }
        for f in &funcs {
            if outer(f.span, &spans) {
                self.func(f);
            }
        }
    }

    fn machine(&mut self, m: &Machine) {
        self.pascal(&m.name, "machine");
        for ev in &m.events {
            self.pascal(&ev.name, "event");
        }
        let events: Vec<&str> = m.events.iter().map(|e| e.name.name.as_str()).collect();
        let mut targeted: Vec<String> = Vec::new();
        self.state_body(&m.body, &events, &mut targeted);
        // The first child of every compound state is entered by default.
        fn defaults(body: &StateBody, out: &mut Vec<String>) {
            if body.initial.is_none()
                && let Some(first) = body.states.first()
            {
                out.push(first.name.name.clone());
            }
            for s in &body.states {
                defaults(&s.body, out);
            }
        }
        defaults(&m.body, &mut targeted);
        fn check(l: &mut Linter, body: &StateBody, targeted: &[String], parallel: bool) {
            for s in &body.states {
                if !parallel && !targeted.contains(&s.name.name) {
                    l.report("unreachable-state", s.name.span, format!("no transition enters state `{}`", s.name.name));
                }
                check(l, &s.body, targeted, s.parallel);
            }
        }
        check(self, &m.body, &targeted, m.parallel);
    }

    fn state_body(&mut self, body: &StateBody, events: &[&str], targeted: &mut Vec<String>) {
        let note = |t: &Transition, targeted: &mut Vec<String>| {
            if let Transition::Target { path, .. } = t {
                for p in path {
                    targeted.push(p.name.clone());
                }
            }
        };
        if let Some(t) = &body.initial {
            note(t, targeted);
        }
        for on in &body.ons {
            if !events.contains(&on.event.name.as_str()) {
                let known = if events.is_empty() {
                    "none are declared".to_string()
                } else {
                    format!("declared events: {}", events.join(", "))
                };
                self.report("unknown-event", on.event.span, format!("unknown event `{}` ({known})", on.event.name));
            }
            note(&on.transition, targeted);
        }
        for inv in &body.invokes {
            for (_, t) in &inv.handlers {
                note(t, targeted);
            }
        }
        for s in &body.states {
            self.pascal(&s.name, "state");
            self.state_body(&s.body, events, targeted);
        }
    }

    fn component(&mut self, c: &Component) {
        self.pascal(&c.name, "component");
        for u in c.updates() {
            self.pascal(&u.message, "message");
            let n = &u.message.name;
            for bad in ["Set", "Update", "Do", "Handle", "Go", "Save", "Load", "Fetch", "Toggle"] {
                if n.starts_with(bad) && n[bad.len()..].starts_with(|ch: char| ch.is_ascii_uppercase()) {
                    self.report(
                        "message-naming",
                        u.message.span,
                        format!("`{n}` reads as a command; name messages for what happened, verb first in the past tense (for example `Clicked{}` or `Updated{}`)", &n[bad.len()..], &n[bad.len()..]),
                    );
                    break;
                }
            }
            match &u.body {
                UpdateBody::Expr(e) => self.expr(e, false),
                UpdateBody::Block(b) => self.block(b, false),
            }
        }
        for e in c.events() {
            self.pascal(&e.name, "event");
        }
        if let Some(v) = c.view() {
            self.expr(v, false);
        }
    }
}

#[cfg(test)]
mod tests {
    fn codes(src: &str) -> Vec<String> {
        let parsed = efs_syntax::parse(src);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        super::lint(src, &parsed.module, &Default::default()).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn discarded_effect() {
        assert_eq!(codes("const p = effect {\n  Console.log(\"hi\")\n}"), vec!["lint/discarded-effect"]);
        assert!(codes("const p = effect {\n  run Console.log(\"hi\")\n}").is_empty());
        assert!(codes("const p = effect {\n  users.set(1, 2)\n}").is_empty());
    }

    #[test]
    fn unused_binding_and_ignore() {
        assert_eq!(codes("effect function f() {\n  const x = run a\n  return 1\n}"), vec!["lint/unused-binding"]);
        assert!(
            codes("effect function f() {\n  // efs-ignore: unused-binding\n  const x = run a\n  return 1\n}")
                .is_empty()
        );
    }

    #[test]
    fn prefer_const() {
        assert_eq!(codes("function f() {\n  let x = 1\n  return x\n}"), vec!["lint/prefer-const"]);
        assert!(codes("function f() {\n  let x = 1\n  x += 1\n  return x\n}").is_empty());
        assert!(codes("function f() {\n  let n = 0\n  for v of [1] {\n    n++\n  }\n  return n\n}").is_empty());
        assert!(codes("function f() {\n  const x = 1\n  return x\n}").is_empty());
    }

    #[test]
    fn machine_rules() {
        let src = "machine M {\n  events { Go }\n  state A {\n    on Stop -> A\n  }\n  state B\n}";
        assert_eq!(codes(src), vec!["lint/unknown-event", "lint/unreachable-state"]);
    }

    #[test]
    fn message_naming() {
        let src = "component C {\n  let n: Number = 0\n  update SetName(name: String) => $.model\n  view => h.div\n}";
        assert_eq!(codes(src), vec!["lint/message-naming"]);
    }
}
