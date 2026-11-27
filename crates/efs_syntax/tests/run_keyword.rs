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
