use std::path::Path;

#[test]
fn examples_parse_without_errors() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/src");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "efs") {
            let src = std::fs::read_to_string(&path).unwrap();
            let result = efs_syntax::parse(&src);
            let index = efs_syntax::LineIndex::new(&src);
            for d in &result.diagnostics {
                let (l, c) = index.position(d.span.start);
                eprintln!("{}:{}:{}: {}", path.display(), l + 1, c + 1, d.message);
            }
            assert!(result.diagnostics.is_empty(), "{} has syntax errors", path.display());
            count += 1;
        }
    }
    assert!(count > 0);
}
