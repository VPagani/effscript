//! Module specifiers: relative `.efs` imports load the compiled module.

fn ts(src: &str) -> String {
    let out = efs_codegen::compile(src, &Default::default());
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.code
}

#[test]
fn dynamic_imports_of_efs_files_load_the_compiled_module() {
    let code = ts("const page = () => import(\"./pages/Home.efs\");\n");
    assert!(code.contains("import(\"./pages/Home.js\")"), "{code}");
    let code = ts("const page = () => import('../Home.efs').then((m) => m.Home);\n");
    assert!(code.contains("import('../Home.js').then"), "{code}");
}

#[test]
fn other_dynamic_imports_are_unchanged() {
    let code = ts("const a = () => import(\"effect\");\nconst b = (p) => import(p);\n");
    assert!(code.contains("import(\"effect\")"), "{code}");
    assert!(code.contains("import(p)"), "{code}");
}
