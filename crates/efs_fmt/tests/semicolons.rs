//! Semicolon placement in both modes.

use efs_fmt::{FormatOptions, format, format_with};

const OFF: FormatOptions = FormatOptions { semicolons: false };

fn on(src: &str) -> String {
    format(src).unwrap()
}

fn off(src: &str) -> String {
    format_with(src, &OFF).unwrap()
}

#[test]
fn default_is_on() {
    assert!(FormatOptions::default().semicolons);
}

#[test]
fn statements_get_semicolons() {
    let src = "import { A } from \"./a.efs\"\nexport const x = 1\ntype T = number\nexport { x }\n\
               const m = match x { 1 => \"a\", _ => \"b\" }\nfoo(1)\n";
    let out = on(src);
    assert!(out.contains("import { A } from \"./a.efs\";\n"), "{out}");
    assert!(out.contains("export const x = 1;\n"), "{out}");
    assert!(out.contains("type T = number;\n"), "{out}");
    assert!(out.contains("export { x };\n"), "{out}");
    assert!(out.contains("};\n") && out.contains("foo(1);\n"), "{out}");
}

#[test]
fn block_declarations_get_none() {
    let src = "function f() { return 1 }\neffect function g() {\n  if a { b() }\n  for x of xs { c(x) }\n  while d { break }\n}\n\
               service S {\n  a(): number\n}\nerror E { m: string }\nschema P { a: number }\nenum K { A, B }\n";
    let out = on(src);
    assert!(!out.contains("};"), "{out}");
    assert!(out.contains("  a(): number;\n"), "{out}");
    assert!(out.contains("return 1;\n"), "{out}");
}

#[test]
fn const_arrow_function_gets_one_but_declaration_does_not() {
    let out = on("function f() { return 1 }\nconst g = () => { return 2 }\n");
    assert_eq!(out, "function f() {\n  return 1;\n}\nconst g = () => {\n  return 2;\n};\n");
}

#[test]
fn trailing_comment_follows_the_semicolon() {
    assert_eq!(on("const x = 1 // note\n"), "const x = 1; // note\n");
}

#[test]
fn machine_and_component_members() {
    let src = "machine M {\n  initial state A {\n    on go -> B\n    after 1000 -> B\n    invoke x { done -> B }\n  }\n  state B\n}\n\
               component C {\n  prop a: number\n  let n: number = 0\n  event e(x: number)\n  update inc { n = n + 1 }\n  view => <div>{n}</div>\n}\n";
    let out = on(src);
    for want in [
        "    on go -> B;\n",
        "    after 1000 -> B;\n",
        "      done -> B;\n",
        "  state B\n",
        "  prop a: number;\n",
        "  let n: number = 0;\n",
        "  event e(x: number);\n",
        "  update inc {\n    n = n + 1;\n  }\n",
        "  view => <div>{n}</div>\n",
    ] {
        assert!(out.contains(want), "missing {want:?} in\n{out}");
    }
}

#[test]
fn off_removes_semicolons() {
    let src = "import { A } from \"./a.efs\";\nconst x = 1; // c\nfunction f() {\n  return 1;\n}\nconst g = () => {\n  return 2;\n};\n\
               component C {\n  prop a: number;\n}\n";
    let out = off(src);
    assert!(!out.contains(';'), "{out}");
    assert!(out.contains("const x = 1 // c\n"), "{out}");
}

#[test]
fn modes_round_trip_and_are_idempotent() {
    let src = "import { A } from \"./a.efs\"\nconst x = 1\nconst m = match x { 1 => \"a\", _ => \"b\" }\n\
               effect function h() {\n  foo(bar)\n  (1)\n  [1, 2]\n  `t`\n  return x\n}\n\
               service S {\n  a(): number\n}\n";
    let a = on(src);
    let b = off(src);
    assert_eq!(on(&a), a);
    assert_eq!(off(&b), b);
    assert_eq!(off(&a), b);
    assert_eq!(on(&b), a);
}

#[test]
fn no_asi_hazard_without_semicolons() {
    // Lines starting with `(`, `[` or a template literal always begin a new statement in
    // EffScript, so dropping `;` is safe; check the compiled output agrees.
    let src = "const a = foo;\n(bar);\nconst b = baz;\n[1, 2];\nconst c = qux;\n`t`;\n";
    let out = off(src);
    assert!(!out.contains(';'), "{out}");
    let opts = efs_codegen::Options::default();
    assert_eq!(efs_codegen::compile(src, &opts).code, efs_codegen::compile(&out, &opts).code);
}

#[test]
fn project_options_come_from_the_nearest_efs_json() {
    let root = std::env::temp_dir().join(format!("efs-fmt-config-{}", std::process::id()));
    let nested = root.join("src/deep");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(root.join("efs.json"), r#"{ "fmt": { "semicolons": false } }"#).unwrap();
    let file = nested.join("a.efs");
    std::fs::write(&file, "").unwrap();
    assert!(!FormatOptions::from_project(&file).semicolons);
    assert!(!FormatOptions::from_project(&nested).semicolons);
    // A closer efs.json wins, even without a `fmt` section.
    std::fs::write(root.join("src/efs.json"), "{}").unwrap();
    assert!(FormatOptions::from_project(&file).semicolons);
    // Invalid JSON falls back to the defaults.
    std::fs::write(root.join("src/efs.json"), "{").unwrap();
    assert!(FormatOptions::from_project(&file).semicolons);
    std::fs::remove_dir_all(&root).unwrap();
}
