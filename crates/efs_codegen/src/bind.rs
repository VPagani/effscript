//! `run` and `use`: both compile to `yield*`, but `use` gets a service and `run` runs an
//! effect, and mixing them up is an error.
//!
//! When the operand is a name declared in the same file, its declaration says whether it is
//! a service, and the error is reported here, so that every tool that compiles EffScript
//! reports it. Other operands (imports, dotted names, calls) are recorded as [`BindSite`]s
//! for the type checker, which looks at their types.

use std::collections::HashMap;

use efs_syntax::ast::*;
use efs_syntax::{Diagnostic, Span};

use crate::BindSite;
use crate::lower::Gen;

/// What a top-level name declared in this file is, as far as `run` and `use` care.
#[derive(Clone, Debug)]
pub(crate) enum DeclKind {
    Service,
    /// Not a service: a description (`a layer`) and advice for `use` on it.
    Other(&'static str, Option<String>),
}

/// A `run` or `use` seen while lowering, resolved once every binding is known.
#[derive(Clone, Debug)]
pub(crate) struct PendingBind {
    pub kind: BindKind,
    pub operand: Span,
    /// The name when the operand is a plain identifier.
    pub name: Option<String>,
    pub gen_pos: u32,
}

#[derive(Default)]
pub(crate) struct Binds {
    /// Top-level declarations by name: where they are declared and what they are.
    pub decls: HashMap<String, (Span, DeclKind)>,
    /// Every name bound by a pattern, parameter or function declaration, with its span.
    pub bound: Vec<(String, Span)>,
    pub pending: Vec<PendingBind>,
}

/// Whether `e` creates a service key: `Context.Service<...>()("Key")`,
/// `Context.Service<Shape>("Key")` or `Context.Reference(...)`.
fn is_service_constructor(e: &Expr) -> bool {
    let mut callee = e;
    let mut called = false;
    while let ExprKind::Call { callee: c, .. } = &callee.kind {
        callee = c;
        called = true;
    }
    called && matches!(callee.name_path().as_deref(), Some("Context.Service" | "Context.Reference"))
}

/// Classifies the value a top-level `const` holds, when its initializer makes that clear.
fn classify_init(name: &str, init: &Expr) -> Option<DeclKind> {
    Some(match &init.kind {
        _ if is_service_constructor(init) => DeclKind::Service,
        ExprKind::Func(f) if f.effect => effect_function(name),
        ExprKind::Func(_) => DeclKind::Other("a function", None),
        ExprKind::EffectBlock(_) | ExprKind::Pipe(_) | ExprKind::With { .. } | ExprKind::Catch { .. } => {
            DeclKind::Other("an effect", Some(format!("run it with `run {name}`")))
        }
        ExprKind::Lit(_) | ExprKind::Template(_) | ExprKind::Array(_) | ExprKind::Object(_) => {
            DeclKind::Other("a value", None)
        }
        _ => return None,
    })
}

fn effect_function(name: &str) -> DeclKind {
    DeclKind::Other("an effect function", Some(format!("call it and run the result with `run {name}(...)`")))
}

impl Binds {
    pub(crate) fn collect(&mut self, module: &Module) {
        for item in &module.items {
            let (id, kind) = match &item.kind {
                ItemKind::Service(s) => (&s.name, DeclKind::Service),
                ItemKind::Layer(l) => {
                    let advice = match l.service.name_path() {
                        Some(s) => format!("provide it with `with {}` and get the service with `use {s}`", l.name.name),
                        None => format!("provide it with `with {}`", l.name.name),
                    };
                    (&l.name, DeclKind::Other("a layer", Some(advice)))
                }
                ItemKind::Error(e) => (
                    &e.name,
                    DeclKind::Other("an error", Some(format!("fail with it with `fail new {}(...)`", e.name.name))),
                ),
                ItemKind::Fn(f) if f.func.effect => (&f.name, effect_function(&f.name.name)),
                ItemKind::Fn(f) => (&f.name, DeclKind::Other("a function", None)),
                ItemKind::Schema(s) => (&s.name, DeclKind::Other("a schema", None)),
                ItemKind::Enum(e) => (&e.name, DeclKind::Other("an enum", None)),
                ItemKind::Machine(m) => (&m.name, DeclKind::Other("a state machine", None)),
                ItemKind::Component(c) => (&c.name, DeclKind::Other("a component", None)),
                ItemKind::Let(l) if l.kind == LetKind::Const => {
                    let (PatternKind::Ident(id), Some(init)) = (&l.pattern.kind, &l.init) else { continue };
                    let Some(kind) = classify_init(&id.name, init) else { continue };
                    (id, kind)
                }
                _ => continue,
            };
            self.decls.insert(id.name.clone(), (id.span, kind));
        }
    }

    /// What the top-level declaration `name` is, unless a local binding shadows it somewhere
    /// in the file.
    fn local_decl(&self, name: &str) -> Option<&DeclKind> {
        let (span, kind) = self.decls.get(name)?;
        let shadowed = self.bound.iter().any(|(n, s)| n == name && s != span);
        (!shadowed).then_some(kind)
    }
}

impl Gen<'_> {
    /// Writes `yield* operand` for `run operand` or `use operand`.
    pub(crate) fn bind(&mut self, span: Span, kind: BindKind, inner: &Expr) {
        let gen_pos = self.w.pos();
        let in_gen = self.in_gen;
        self.yield_star(span, kind);
        if in_gen {
            let name = match &inner.kind {
                ExprKind::Ident(n) => Some(n.clone()),
                _ => None,
            };
            self.binds.pending.push(PendingBind { kind, operand: inner.span, name, gen_pos });
        }
        self.expr_top(inner);
    }

    /// Records a name bound by a pattern or a function declaration.
    pub(crate) fn bound(&mut self, id: &Ident) {
        self.binds.bound.push((id.name.clone(), id.span));
    }

    /// Reports `run` on a local service and `use` on a local non-service, and returns the
    /// sites the type checker has to decide.
    pub(crate) fn resolve_binds(&mut self) -> Vec<BindSite> {
        let pending = std::mem::take(&mut self.binds.pending);
        let mut sites = Vec::new();
        for p in pending {
            let decl = p.name.as_deref().and_then(|n| self.binds.local_decl(n).cloned());
            let name = p.name.as_deref().unwrap_or_default();
            match (p.kind, decl) {
                (BindKind::Run, Some(DeclKind::Service)) => self.diags.push(
                    Diagnostic::error(p.operand, format!("`{name}` is a service; get it with `use {name}`"))
                        .with_code("run-service"),
                ),
                (BindKind::Use, Some(DeclKind::Other(what, advice))) => {
                    let advice = advice.unwrap_or_else(|| "`use` only gets services".into());
                    self.diags.push(
                        Diagnostic::error(p.operand, format!("`{name}` is {what}, not a service; {advice}"))
                            .with_code("use-non-service"),
                    );
                }
                (_, Some(_)) => {}
                (kind, None) => sites.push(BindSite { kind, operand: p.operand, gen_pos: p.gen_pos }),
            }
        }
        sites
    }
}
