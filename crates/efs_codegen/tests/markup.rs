//! Markup attributes compile to Effect UI's builder methods where the element has them.

fn ts(src: &str) -> String {
    let out = efs_codegen::compile(src, &Default::default());
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.code
}

#[test]
fn controlled_properties_only_on_elements_that_have_them() {
    let code = ts("const v = (h) => <select value={a}><option value=\"x\" selected={b}>X</option></select>;\n");
    assert!(code.contains("h.select.value(a)"), "{code}");
    assert!(code.contains("h.option.attr(\"value\", \"x\").selected(b)"), "{code}");
    let code = ts("const v = (h) => <input value={a} checked={c} disabled={d} />;\n");
    assert!(code.contains("h.input.value(a).checked(c).disabled(d)"), "{code}");
    let code = ts("const v = (h) => <li value=\"1\">x</li>;\n");
    assert!(code.contains("h.li.attr(\"value\", \"1\")"), "{code}");
}

#[test]
fn components_without_messages_still_have_an_update() {
    let code = ts("component Banner {\n  prop text: String;\n  view => <p>{text}</p>\n}\n");
    assert!(code.contains("update: ($) => $.model"), "{code}");
    let code = ts("component Banner {\n  update = ($) => $.model;\n  view => <p>hi</p>\n}\n");
    assert_eq!(code.matches("update:").count(), 1, "{code}");
}
