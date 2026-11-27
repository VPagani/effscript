//! Read-only traversal helpers.

use crate::ast::*;

/// Calls `f` for `e` and every expression nested in it, including those in
/// statements, function bodies and markup.
pub fn walk_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Ident(_) | ExprKind::Lit(_) | ExprKind::Error => {}
        ExprKind::Template(parts) => {
            for p in parts {
                if let TemplatePart::Expr(x) = p {
                    walk_expr(x, f);
                }
            }
        }
        ExprKind::Array(elems) => elems.iter().for_each(|x| walk_elem(x, f)),
        ExprKind::Object(props) => {
            for p in props {
                match p {
                    Prop::KeyValue { key, value } => {
                        if let PropKey::Computed(k) = key {
                            walk_expr(k, f);
                        }
                        walk_expr(value, f);
                    }
                    Prop::Shorthand(id) => f(&Expr { kind: ExprKind::Ident(id.name.clone()), span: id.span }),
                    Prop::Spread(x) => walk_expr(x, f),
                    Prop::Method { func, .. } => walk_func(func, f),
                }
            }
        }
        ExprKind::Paren(x) | ExprKind::NonNull(x) | ExprKind::Bind(x) => walk_expr(x, f),
        ExprKind::Unary { expr, .. } | ExprKind::Update { expr, .. } => walk_expr(expr, f),
        ExprKind::As { expr, .. } | ExprKind::Satisfies { expr, .. } => walk_expr(expr, f),
        ExprKind::Binary { left, right, .. } => {
            walk_expr(left, f);
            walk_expr(right, f);
        }
        ExprKind::Assign { target, value, .. } => {
            walk_expr(target, f);
            walk_expr(value, f);
        }
        ExprKind::Cond { cond, then, else_ } | ExprKind::If { cond, then, else_ } => {
            walk_expr(cond, f);
            walk_expr(then, f);
            walk_expr(else_, f);
        }
        ExprKind::Call { callee, args, .. } | ExprKind::New { callee, args, .. } => {
            walk_expr(callee, f);
            args.iter().for_each(|x| walk_elem(x, f));
        }
        ExprKind::Member { object, .. } => walk_expr(object, f),
        ExprKind::Index { object, index, .. } => {
            walk_expr(object, f);
            walk_expr(index, f);
        }
        ExprKind::Func(func) => walk_func(func, f),
        ExprKind::Pipe(parts) => parts.iter().for_each(|x| walk_expr(x, f)),
        ExprKind::With { expr, layer } => {
            walk_expr(expr, f);
            walk_expr(layer, f);
        }
        ExprKind::Catch { expr, handler } => {
            walk_expr(expr, f);
            match handler {
                CatchHandler::Tags(arms) => arms.iter().for_each(|a| walk_expr(&a.body, f)),
                CatchHandler::All { body, .. } => walk_expr(body, f),
            }
        }
        ExprKind::Match { expr, arms } => {
            walk_expr(expr, f);
            arms.iter().for_each(|a| walk_expr(&a.body, f));
        }
        ExprKind::EffectBlock(b) => walk_block(b, f),
        ExprKind::Markup(el) => walk_element(el, f),
    }
}

fn walk_elem(e: &ArrayElem, f: &mut dyn FnMut(&Expr)) {
    match e {
        ArrayElem::Expr(x) | ArrayElem::Spread(x) => walk_expr(x, f),
        ArrayElem::Hole => {}
    }
}

pub fn walk_element(el: &Element, f: &mut dyn FnMut(&Expr)) {
    for a in &el.attrs {
        match a {
            Attr::Named { value: Some(AttrValue::Expr(x)), .. } | Attr::Spread(x) => walk_expr(x, f),
            _ => {}
        }
    }
    for c in &el.children {
        match c {
            Child::Expr(x) => walk_expr(x, f),
            Child::Element(e) => walk_element(e, f),
            Child::Text(..) => {}
        }
    }
}

pub fn walk_func(func: &Func, f: &mut dyn FnMut(&Expr)) {
    for p in &func.params {
        if let Some(d) = &p.default {
            walk_expr(d, f);
        }
    }
    match &func.body {
        FuncBody::Expr(x) => walk_expr(x, f),
        FuncBody::Block(b) => walk_block(b, f),
    }
}

pub fn walk_block(b: &Block, f: &mut dyn FnMut(&Expr)) {
    for s in &b.stmts {
        walk_stmt(s, f);
    }
}

pub fn walk_stmt(s: &Stmt, f: &mut dyn FnMut(&Expr)) {
    match &s.kind {
        StmtKind::Let(l) => {
            if let Some(i) = &l.init {
                walk_expr(i, f);
            }
        }
        StmtKind::Fn(d) => walk_func(&d.func, f),
        StmtKind::Expr(e) | StmtKind::Fail(e) | StmtKind::Defer(e) | StmtKind::Throw(e) | StmtKind::Emit(e) => {
            walk_expr(e, f)
        }
        StmtKind::Return(e) => {
            if let Some(e) = e {
                walk_expr(e, f);
            }
        }
        StmtKind::If(i) => walk_if(i, f),
        StmtKind::For { iter, body, .. } => {
            walk_expr(iter, f);
            walk_block(body, f);
        }
        StmtKind::While { cond, body } => {
            walk_expr(cond, f);
            walk_block(body, f);
        }
        StmtKind::Block(b) => walk_block(b, f),
        StmtKind::Break | StmtKind::Continue => {}
    }
}

fn walk_if(i: &IfStmt, f: &mut dyn FnMut(&Expr)) {
    walk_expr(&i.cond, f);
    walk_block(&i.then, f);
    if let Some(e) = &i.else_ {
        walk_stmt(e, f);
    }
}

/// Names of identifiers referenced anywhere in `e` (not scope-aware).
pub fn referenced_names(e: &Expr) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    walk_expr(e, &mut |x| {
        if let ExprKind::Ident(n) = &x.kind
            && !out.contains(n)
        {
            out.push(n.clone());
        }
    });
    out
}
