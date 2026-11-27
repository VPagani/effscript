//! Read-only traversal helpers.

use crate::ast::*;

/// Calls `f` for `e` and every expression nested in it, including those in
/// statements, function bodies and markup.
pub fn walk_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    walk_expr_pruned(e, &mut |x| {
        f(x);
        true
    });
}

pub fn walk_block(b: &Block, f: &mut dyn FnMut(&Expr)) {
    walk_block_pruned(b, &mut |x| {
        f(x);
        true
    });
}

/// Like [`walk_expr`], but `f` returns whether to descend into the expression it was given.
pub fn walk_expr_pruned(e: &Expr, f: &mut dyn FnMut(&Expr) -> bool) {
    if !f(e) {
        return;
    }
    match &e.kind {
        ExprKind::Ident(_) | ExprKind::Lit(_) | ExprKind::Error => {}
        ExprKind::Template(parts) => {
            for p in parts {
                if let TemplatePart::Expr(x) = p {
                    walk_expr_pruned(x, f);
                }
            }
        }
        ExprKind::Array(elems) => elems.iter().for_each(|x| walk_elem(x, f)),
        ExprKind::Object(props) => {
            for p in props {
                match p {
                    Prop::KeyValue { key, value } => {
                        if let PropKey::Computed(k) = key {
                            walk_expr_pruned(k, f);
                        }
                        walk_expr_pruned(value, f);
                    }
                    Prop::Shorthand(id) => {
                        f(&Expr { kind: ExprKind::Ident(id.name.clone()), span: id.span });
                    }
                    Prop::Spread(x) => walk_expr_pruned(x, f),
                    Prop::Method { func, .. } => {
                        // Methods are visited as function expressions so that they can be pruned.
                        let as_expr = Expr { kind: ExprKind::Func(Box::new(func.clone())), span: func.span };
                        if f(&as_expr) {
                            walk_func(func, f);
                        }
                    }
                }
            }
        }
        ExprKind::Paren(x) | ExprKind::NonNull(x) | ExprKind::Bind(x) => walk_expr_pruned(x, f),
        ExprKind::Unary { expr, .. } | ExprKind::Update { expr, .. } => walk_expr_pruned(expr, f),
        ExprKind::As { expr, .. } | ExprKind::Satisfies { expr, .. } => walk_expr_pruned(expr, f),
        ExprKind::Binary { left, right, .. } => {
            walk_expr_pruned(left, f);
            walk_expr_pruned(right, f);
        }
        ExprKind::Assign { target, value, .. } => {
            walk_expr_pruned(target, f);
            walk_expr_pruned(value, f);
        }
        ExprKind::Cond { cond, then, else_ } | ExprKind::If { cond, then, else_ } => {
            walk_expr_pruned(cond, f);
            walk_expr_pruned(then, f);
            walk_expr_pruned(else_, f);
        }
        ExprKind::Call { callee, args, .. } | ExprKind::New { callee, args, .. } => {
            walk_expr_pruned(callee, f);
            args.iter().for_each(|x| walk_elem(x, f));
        }
        ExprKind::Member { object, .. } => walk_expr_pruned(object, f),
        ExprKind::Index { object, index, .. } => {
            walk_expr_pruned(object, f);
            walk_expr_pruned(index, f);
        }
        ExprKind::Func(func) => walk_func(func, f),
        ExprKind::Pipe(parts) => parts.iter().for_each(|x| walk_expr_pruned(x, f)),
        ExprKind::With { expr, layer } => {
            walk_expr_pruned(expr, f);
            walk_expr_pruned(layer, f);
        }
        ExprKind::Catch { expr, handler } => {
            walk_expr_pruned(expr, f);
            match handler {
                CatchHandler::Tags(arms) => arms.iter().for_each(|a| walk_expr_pruned(&a.body, f)),
                CatchHandler::All { body, .. } => walk_expr_pruned(body, f),
            }
        }
        ExprKind::Match { expr, arms } => {
            walk_expr_pruned(expr, f);
            arms.iter().for_each(|a| walk_expr_pruned(&a.body, f));
        }
        ExprKind::EffectBlock(b) => walk_block_pruned(b, f),
        ExprKind::Markup(el) => walk_element(el, f),
    }
}

fn walk_elem(e: &ArrayElem, f: &mut dyn FnMut(&Expr) -> bool) {
    match e {
        ArrayElem::Expr(x) | ArrayElem::Spread(x) => walk_expr_pruned(x, f),
        ArrayElem::Hole => {}
    }
}

pub fn walk_element(el: &Element, f: &mut dyn FnMut(&Expr) -> bool) {
    for a in &el.attrs {
        match a {
            Attr::Named { value: Some(AttrValue::Expr(x)), .. } | Attr::Spread(x) => walk_expr_pruned(x, f),
            _ => {}
        }
    }
    for c in &el.children {
        match c {
            Child::Expr(x) => walk_expr_pruned(x, f),
            Child::Element(e) => walk_element(e, f),
            Child::Text(..) => {}
        }
    }
}

pub fn walk_func(func: &Func, f: &mut dyn FnMut(&Expr) -> bool) {
    for p in &func.params {
        if let Some(d) = &p.default {
            walk_expr_pruned(d, f);
        }
    }
    match &func.body {
        FuncBody::Expr(x) => walk_expr_pruned(x, f),
        FuncBody::Block(b) => walk_block_pruned(b, f),
    }
}

pub fn walk_block_pruned(b: &Block, f: &mut dyn FnMut(&Expr) -> bool) {
    for s in &b.stmts {
        walk_stmt(s, f);
    }
}

pub fn walk_stmt(s: &Stmt, f: &mut dyn FnMut(&Expr) -> bool) {
    match &s.kind {
        StmtKind::Let(l) => {
            if let Some(i) = &l.init {
                walk_expr_pruned(i, f);
            }
        }
        StmtKind::Fn(d) => walk_func(&d.func, f),
        StmtKind::Expr(e) | StmtKind::Fail(e) | StmtKind::Defer(e) | StmtKind::Throw(e) | StmtKind::Emit(e) => {
            walk_expr_pruned(e, f)
        }
        StmtKind::Return(e) => {
            if let Some(e) = e {
                walk_expr_pruned(e, f);
            }
        }
        StmtKind::If(i) => walk_if(i, f),
        StmtKind::For { iter, body, .. } => {
            walk_expr_pruned(iter, f);
            walk_block_pruned(body, f);
        }
        StmtKind::While { cond, body } => {
            walk_expr_pruned(cond, f);
            walk_block_pruned(body, f);
        }
        StmtKind::Block(b) => walk_block_pruned(b, f),
        StmtKind::Break | StmtKind::Continue => {}
    }
}

fn walk_if(i: &IfStmt, f: &mut dyn FnMut(&Expr) -> bool) {
    walk_expr_pruned(&i.cond, f);
    walk_block_pruned(&i.then, f);
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
