//! Compiles every fixture and compares with `tests/snapshots/*.ts`.
//! Run with `UPDATE_SNAPSHOTS=1` to accept new output.

use std::path::{Path, PathBuf};

use efs_codegen::SegmentKind;

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Vec::new();
    for dir in ["tests/fixtures", "examples/src"] {
        for e in std::fs::read_dir(root.join(dir)).unwrap().flatten() {
            if e.path().extension().is_some_and(|x| x == "efs") {
                out.push(e.path());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn snapshots() {
    let snap_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/snapshots");
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let mut failures = Vec::new();
    for path in fixtures() {
        let src = std::fs::read_to_string(&path).unwrap();
        let out = efs_codegen::compile(&src, &Default::default());
        assert!(out.diagnostics.is_empty(), "{}: {:#?}", path.display(), out.diagnostics);
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let snap = snap_dir.join(format!("{name}.ts"));
        if update {
            std::fs::write(&snap, &out.code).unwrap();
            continue;
        }
        match std::fs::read_to_string(&snap) {
            Ok(expected) if expected == out.code => {}
            Ok(_) => failures.push(format!("{name}: output differs from {}", snap.display())),
            Err(_) => failures.push(format!("{name}: missing snapshot {}", snap.display())),
        }
    }
    assert!(failures.is_empty(), "{}\n(run with UPDATE_SNAPSHOTS=1 to accept)", failures.join("\n"));
}

#[test]
fn span_maps_are_valid() {
    for path in fixtures() {
        let src = std::fs::read_to_string(&path).unwrap();
        let out = efs_codegen::compile(&src, &Default::default());
        let mut prev_end = 0;
        for s in &out.segments {
            assert!(s.gen_start >= prev_end, "{}: overlapping segments at {}", path.display(), s.gen_start);
            prev_end = s.gen_start + s.gen_len;
            assert!(prev_end as usize <= out.code.len());
            assert!((s.src_start + s.src_len) as usize <= src.len());
            if s.kind == SegmentKind::Verbatim {
                assert_eq!(
                    &out.code[s.gen_start as usize..prev_end as usize],
                    &src[s.src_start as usize..(s.src_start + s.src_len) as usize],
                    "{}: verbatim segment differs",
                    path.display()
                );
            }
        }
        // Every identifier in the source maps back to itself.
        for (i, _) in src.match_indices("const ") {
            let p = i as u32 + 6;
            if let Some(g) = out.to_generated(p) {
                let back = out.to_source(g).unwrap();
                assert!(back.span.contains(p), "{}: {p} does not round-trip", path.display());
            }
        }
    }
}

#[test]
fn javascript_snapshots() {
    let snap_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/snapshots/js");
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let opts = js_options();
    let node = std::process::Command::new("node").arg("--version").output().is_ok();
    let tmp = std::env::temp_dir().join(format!("efs-js-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut failures = Vec::new();
    for path in fixtures() {
        let src = std::fs::read_to_string(&path).unwrap();
        let out = efs_codegen::compile(&src, &opts);
        assert!(out.diagnostics.is_empty(), "{}: {:#?}", path.display(), out.diagnostics);
        for s in &out.segments {
            if s.kind == SegmentKind::Verbatim {
                assert_eq!(
                    &out.code[s.gen_start as usize..(s.gen_start + s.gen_len) as usize],
                    &src[s.src_start as usize..(s.src_start + s.src_len) as usize]
                );
            }
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        if node {
            let file = tmp.join(format!("{name}.mjs"));
            std::fs::write(&file, &out.code).unwrap();
            let check = std::process::Command::new("node").arg("--check").arg(&file).output().unwrap();
            assert!(
                check.status.success(),
                "{name}: not valid JavaScript:\n{}",
                String::from_utf8_lossy(&check.stderr)
            );
        }
        let snap = snap_dir.join(format!("{name}.js"));
        if update {
            std::fs::write(&snap, &out.code).unwrap();
        } else if std::fs::read_to_string(&snap).ok().as_deref() != Some(out.code.as_str()) {
            failures.push(format!("{name}: output differs from {}", snap.display()));
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(failures.is_empty(), "{}\n(run with UPDATE_SNAPSHOTS=1 to accept)", failures.join("\n"));
}

fn js_options() -> efs_codegen::Options {
    efs_codegen::Options { target: efs_codegen::Target::JavaScript, ..Default::default() }
}
