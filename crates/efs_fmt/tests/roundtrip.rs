//! Formatting must be idempotent, keep every comment, and never change the compiled output.

use std::path::Path;

fn sources() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in ["../../examples/src", "../../tests/fixtures"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "efs") {
                out.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    out
}

fn comments(src: &str) -> Vec<String> {
    efs_syntax::parse(src).comments.iter().map(|c| c.span.text(src).trim().to_string()).collect()
}

#[test]
fn format_is_stable_and_preserves_semantics() {
    let opts = efs_codegen::Options::default();
    for (name, src) in sources() {
        let Ok(once) = efs_fmt::format(&src) else { continue };
        let twice =
            efs_fmt::format(&once).unwrap_or_else(|e| panic!("{name}: formatted output does not parse: {e:?}\n{once}"));
        assert_eq!(once, twice, "{name}: formatting is not idempotent");
        assert_eq!(comments(&src), comments(&once), "{name}: comments changed");
        let a = efs_codegen::compile(&src, &opts).code;
        let b = efs_codegen::compile(&once, &opts).code;
        assert_eq!(a, b, "{name}: formatting changed the compiled output\n--- formatted ---\n{once}");
    }
}

#[test]
fn use_is_printed_before_its_name() {
    let src = "effect function f() {\n  const repo = use   UserRepo\n  const client = use Http.Client\n  return (use  Clock).now\n}\nconst use = 1\nconst x = use + 1\n";
    let once = efs_fmt::format(src).unwrap();
    for line in
        ["const repo = use UserRepo", "const client = use Http.Client", "return (use Clock).now", "const x = use + 1"]
    {
        assert!(once.contains(line), "missing `{line}` in\n{once}");
    }
    assert_eq!(efs_fmt::format(&once).unwrap(), once);
}
