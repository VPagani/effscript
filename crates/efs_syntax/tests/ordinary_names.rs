//! `layer` is an ordinary name, and `=` keeps its TypeScript meanings.

fn errors(src: &str) -> Vec<String> {
    efs_syntax::parse(src).diagnostics.into_iter().map(|d| d.message).collect()
}

#[test]
fn layer_is_an_ordinary_name() {
    let src = "const layer = makeLayer();\nconst a = Mailer.layer;\nconst b = layer(layer);\nconst c = { layer };\nexport const main = program with layer;\n";
    assert!(errors(src).is_empty(), "{:?}", errors(src));
}

#[test]
fn other_equals_forms_stay() {
    let src = "type A = number;\nschema Id = String & Brand<\"Id\">;\nconst f = (x: number) => x;\ncomponent C {\n  init = Effect.void\n  view => h.div\n}\n";
    assert!(errors(src).is_empty(), "{:?}", errors(src));
}
