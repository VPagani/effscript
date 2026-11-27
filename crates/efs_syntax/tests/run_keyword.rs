fn errors(src: &str) -> Vec<String> {
    efs_syntax::parse(src).diagnostics.into_iter().map(|d| d.message).collect()
}

#[test]
fn run_is_a_keyword_only_in_effect_bodies() {
    assert!(errors("effect function f() {\n  const x = run a |> b\n  run c\n  return (run d) + x\n}").is_empty());
    assert!(errors("const f = effect (x) => run g(x)\nconst r = run(1)\nconst run2 = run").is_empty());
    let e = errors("function f() {\n  const x = run a\n}");
    assert_eq!(e.len(), 1);
    assert!(e[0].contains("only be used inside"), "{e:?}");
}

#[test]
fn nested_run_needs_parentheses() {
    let e = errors("effect function f() {\n  return run a + run b\n}");
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("parentheses"));
}

#[test]
fn plain_functions_inside_effects_are_not_effect_bodies() {
    let e = errors("effect function f() {\n  const g = (x) => run x\n}");
    assert_eq!(e.len(), 1, "{e:?}");
}

#[test]
fn less_than_minus_is_a_comparison_again() {
    assert!(errors("const b = x<-1").is_empty());
}

#[test]
fn const_and_let_follow_javascript() {
    assert!(errors("const a = 1\nlet b = 2\nlet c\nfor let i of xs {\n  c = i\n}").is_empty());
    let e = errors("const a\n");
    assert!(e[0].contains("`const` bindings need a value"), "{e:?}");
    let e = errors("var a = 1\n");
    assert!(e[0].contains("`var` is not supported"), "{e:?}");
}

#[test]
fn leading_operators_continue_the_previous_line() {
    let src = "const total = a\n  + b\n  - c\n  * d\nconst next = (x)\n";
    let parsed = efs_syntax::parse(src);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.module.items.len(), 2);
    // A line starting with `(` or `[` still begins a new statement.
    let parsed = efs_syntax::parse("const f = g\n(1)\n");
    assert_eq!(parsed.module.items.len(), 2);
}

fn bind_kinds(src: &str) -> Vec<(efs_syntax::ast::BindKind, String)> {
    use efs_syntax::ast::{ExprKind, ItemKind};
    let parsed = efs_syntax::parse(src);
    let mut out = Vec::new();
    for item in &parsed.module.items {
        let ItemKind::Fn(f) = &item.kind else { continue };
        let efs_syntax::ast::FuncBody::Block(b) = &f.func.body else { continue };
        efs_syntax::visit::walk_block(b, &mut |e| {
            if let ExprKind::Bind(kind, inner) = &e.kind {
                out.push((*kind, inner.span.text(src).to_string()));
            }
        });
    }
    out
}

#[test]
fn use_gets_a_service_by_name() {
    use efs_syntax::ast::BindKind::{Run, Use};
    let src = "effect function f() {\n  const repo = use UserRepo\n  const client = use Http.Client\n  const x = run repo.find(1)\n  return (use Clock).now\n}";
    assert!(errors(src).is_empty(), "{:?}", errors(src));
    assert_eq!(
        bind_kinds(src),
        vec![
            (Use, "UserRepo".to_string()),
            (Use, "Http.Client".to_string()),
            (Run, "repo.find(1)".to_string()),
            (Use, "Clock".to_string()),
        ]
    );
}

#[test]
fn use_is_an_ordinary_name_unless_a_name_follows() {
    let src = "function use(x: number): number {\n  return x + 1\n}\nconst a = use(1)\nconst use2 = use\nconst b = x.use\nconst o = { use: 1 }\neffect function f() {\n  const c = use(2)\n  const d = o.use\n  use\n  return use as number\n}";
    assert!(errors(src).is_empty(), "{:?}", errors(src));
    assert!(bind_kinds(src).is_empty());
    assert!(errors("const use = 1\nconst x = use + 1").is_empty());
}

#[test]
fn use_takes_only_a_name() {
    let e = errors("effect function f() {\n  const x = use repo.find(id)\n}");
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("`use` takes the name of a service"), "{e:?}");
    assert!(e[0].contains("write `run repo.find(id)`"), "{e:?}");
    let e = errors("effect function f() {\n  const x = use Repo |> Effect.orDie\n}");
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("write `run Repo |> Effect.orDie`"), "{e:?}");
}

#[test]
fn use_is_only_allowed_in_effect_bodies() {
    let e = errors("function f() {\n  const x = use Repo\n}");
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("`use` can only be used inside"), "{e:?}");
    let e = errors("effect function f() {\n  const g = () => use Repo\n}");
    assert_eq!(e.len(), 1, "{e:?}");
    // Layer bodies are effect bodies.
    assert!(errors("layer RepoLive for Repo {\n  const sql = use Sql\n  return { all: sql.all }\n}").is_empty());
    let e = errors("component C {\n  update Clicked {\n    use Repo\n  }\n  view => h.div\n}");
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("an update cannot get services"), "{e:?}");
}

#[test]
fn nested_use_needs_parentheses() {
    let e = errors("effect function f() {\n  return 1 + use Repo\n}");
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("wrap `use ...` in parentheses"), "{e:?}");
}
