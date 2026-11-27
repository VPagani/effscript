//! Diagnostics for component declarations.

fn errors(body: &str) -> Vec<String> {
    let src = format!("component C {{\n{body}\n  view => h.div\n}}\n");
    efs_codegen::compile(&src, &Default::default()).diagnostics.into_iter().map(|d| d.message).collect()
}

fn has(errors: &[String], needle: &str) -> bool {
    errors.iter().any(|e| e.contains(needle))
}

#[test]
fn props_are_read_only() {
    let e = errors("  prop label: String\n  update Clicked {\n    label = \"x\"\n  }");
    assert!(has(&e, "is a prop and cannot be assigned"), "{e:?}");
}

#[test]
fn views_cannot_assign() {
    let src = "component C {\n  let n: Number = 0\n  view => h.button.on(\"click\", () => { n = 1 })\n}\n";
    let e: Vec<String> =
        efs_codegen::compile(src, &Default::default()).diagnostics.into_iter().map(|d| d.message).collect();
    assert!(has(&e, "the view cannot change `n`"), "{e:?}");
}

#[test]
fn update_blocks_have_no_return() {
    let e = errors("  let n: Number = 0\n  update Clicked {\n    return n\n  }");
    assert!(has(&e, "ends by itself"), "{e:?}");
}

#[test]
fn commands_only_at_the_top_of_an_update() {
    let e = errors("  let n: Number = 0\n  update Clicked {\n    if n > 0 {\n      run Save.make({ n })\n    }\n  }");
    assert!(has(&e, "must be a statement of its own"), "{e:?}");
}

#[test]
fn bare_message_names_need_no_fields() {
    let src =
        "component C {\n  update Added(n: Number) => $.model\n  view => <button on:click={Added}>Add</button>\n}\n";
    let e: Vec<String> =
        efs_codegen::compile(src, &Default::default()).diagnostics.into_iter().map(|d| d.message).collect();
    assert!(has(&e, "`Added` has fields"), "{e:?}");
}

#[test]
fn model_fields_need_initial_values() {
    let e = errors("  let n: Number");
    assert!(has(&e, "give `n` an initial value"), "{e:?}");
}

#[test]
fn ambiguous_names() {
    let e = errors("  prop n: Number\n  let n: Number = 0\n  update Clicked(n: Number) => $.model\n  event Clicked");
    assert!(has(&e, "already declared as a prop"), "{e:?}");
    assert!(has(&e, "is both a message and an event"), "{e:?}");
}

fn machine_errors(body: &str) -> Vec<String> {
    let src = format!("machine M {{\n  events {{ Go }}\n{body}\n}}\n");
    efs_codegen::compile(&src, &Default::default()).diagnostics.into_iter().map(|d| d.message).collect()
}

#[test]
fn initial_state_rules() {
    let e = machine_errors("  initial state A\n  initial state B");
    assert!(has(&e, "`B` is not the only initial state"), "{e:?}");
    let e = machine_errors("  initial state A(n: Number)");
    assert!(has(&e, "`n` needs a default value"), "{e:?}");
    let e = machine_errors("  initial state A {\n    on Go -> B { n: 1 }\n  }\n  state B(n: Number = 0)");
    assert!(has(&e, "field defaults are the starting data of the initial state"), "{e:?}");
    let out = efs_codegen::compile(
        "machine M {\n  input Number\n  events { Go }\n  initial state A(n: Number = input, m: Number = 2)\n}\n",
        &Default::default(),
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(
        out.code.contains(r#"initial: { target: "A", data: ({ input }) => ({ n: input, m: 2 }) }"#),
        "{}",
        out.code
    );
}
