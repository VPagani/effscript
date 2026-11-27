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
fn old_block_syntax_explains_the_new_one() {
    let e = errors("  model { n: Number }");
    assert!(has(&e, "declare each model field with `let`"), "{e:?}");
}

#[test]
fn ambiguous_names() {
    let e = errors("  prop n: Number\n  let n: Number = 0\n  update Clicked(n: Number) => $.model\n  event Clicked");
    assert!(has(&e, "already declared as a prop"), "{e:?}");
    assert!(has(&e, "is both a message and an event"), "{e:?}");
}
