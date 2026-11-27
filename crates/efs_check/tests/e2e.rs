//! Type checks the examples and fixtures against the real `effect`,
//! `@typeonce/effect-machine` and `@effect/ui` packages. Skipped unless
//! `examples/node_modules` is installed (`pnpm install` at the repository root).

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn check_dir(dir: &Path) -> Vec<String> {
    let mut session = efs_check::Session::new(efs_check::Workspace::discover(dir));
    let mut diags = session.efs_diagnostics();
    diags.extend(session.type_diagnostics(false));
    diags
        .iter()
        .map(|d| {
            let (l, c) = d.line_col().unwrap_or((0, 0));
            format!(
                "{}:{}:{}: {} [{}]",
                d.file.as_ref().map(|f| f.display().to_string()).unwrap_or_default(),
                l + 1,
                c + 1,
                d.message,
                d.code
            )
        })
        .collect()
}

#[test]
fn examples_and_fixtures_type_check() {
    let root = root();
    let modules = root.join("examples/node_modules");
    if !modules.join("effect").exists() {
        eprintln!("skipping: run `pnpm install` at the repository root to enable");
        return;
    }
    let examples = check_dir(&root.join("examples"));
    assert!(examples.is_empty(), "examples:\n{}", examples.join("\n"));

    // Fixtures run as a throwaway project that borrows the examples' dependencies.
    let tmp = std::env::temp_dir().join(format!("efs-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("src")).unwrap();
    std::fs::copy(root.join("examples/tsconfig.json"), tmp.join("tsconfig.json")).unwrap();
    std::fs::copy(root.join("examples/package.json"), tmp.join("package.json")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&modules, tmp.join("node_modules")).unwrap();
    for e in std::fs::read_dir(root.join("tests/fixtures")).unwrap().flatten() {
        if e.path().extension().is_some_and(|x| x == "efs") {
            std::fs::copy(e.path(), tmp.join("src").join(e.file_name())).unwrap();
        }
    }
    let fixtures = check_dir(&tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(fixtures.is_empty(), "fixtures:\n{}", fixtures.join("\n"));
}

#[test]
fn errors_map_back_to_efs_source() {
    let root = root();
    let modules = root.join("examples/node_modules");
    if !modules.join("effect").exists() {
        return;
    }
    let tmp = std::env::temp_dir().join(format!("efs-e2e-err-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::copy(root.join("examples/tsconfig.json"), tmp.join("tsconfig.json")).unwrap();
    std::fs::write(
        tmp.join("tsconfig.json"),
        std::fs::read_to_string(root.join("examples/tsconfig.json"))
            .unwrap()
            .replace("\"include\": [\"src\"]", "\"include\": [\".\"]"),
    )
    .unwrap();
    std::fs::copy(root.join("examples/package.json"), tmp.join("package.json")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&modules, tmp.join("node_modules")).unwrap();
    let src = "import { Effect } from \"effect\"\n\neffect function double(n: number) {\n  return n * 2\n}\n\nexport const bad = effect {\n  const x = run double(\"two\")\n  return x\n}\n";
    std::fs::write(tmp.join("main.efs"), src).unwrap();
    let mut session = efs_check::Session::new(efs_check::Workspace::discover(&tmp));
    let diags = session.type_diagnostics(true);
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(diags.len(), 1, "{diags:#?}");
    let d = &diags[0];
    assert_eq!(d.code, "ts2345");
    let span = d.span.unwrap();
    assert_eq!(&src[span.start as usize..span.end as usize], "\"two\"");
    assert_eq!(d.line_col(), Some((7, 23)));
}

#[test]
fn handwritten_typescript_is_never_shadowed() {
    let tmp = std::env::temp_dir().join(format!("efs-conflict-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("tsconfig.json"), "{ \"include\": [\".\"] }").unwrap();
    std::fs::write(tmp.join("same.efs"), "export const a = 1\n").unwrap();
    std::fs::write(tmp.join("same.ts"), "export const a = 2\n").unwrap();
    let mut session = efs_check::Session::new(efs_check::Workspace::discover(&tmp));
    let diags = session.efs_diagnostics();
    // A file that efs generated may be replaced.
    std::fs::write(tmp.join("same.ts"), efs_check::generated_header("same.efs") + "export const a = 1\n").unwrap();
    session.invalidate();
    let after = session.efs_diagnostics();
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].code, "conflict");
    assert!(after.is_empty(), "{after:#?}");
}

#[test]
fn effect_language_service_diagnostics_map_to_efs() {
    let root = root();
    let modules = root.join("examples/node_modules");
    let Some(exe) = efs_check::effect_tsgo::find(&root.join("examples")) else {
        eprintln!("skipping: effect-tsgo is not installed in examples/");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("efs-effect-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(
        tmp.join("tsconfig.json"),
        std::fs::read_to_string(root.join("examples/tsconfig.json"))
            .unwrap()
            .replace("\"include\": [\"src\"]", "\"include\": [\".\"]"),
    )
    .unwrap();
    std::fs::copy(root.join("examples/package.json"), tmp.join("package.json")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&modules, tmp.join("node_modules")).unwrap();
    let src = "import { Effect, Layer } from \"effect\"\n\nexport const program = Effect.succeed(1)\n  with Layer.empty\n  with Layer.empty\n";
    std::fs::write(tmp.join("main.efs"), src).unwrap();
    let mut session = efs_check::Session::new(efs_check::Workspace::discover(&tmp));
    let diags = session.effect_diagnostics(&exe).unwrap();
    let leftover = tmp.join(".efs-effect.tsconfig.json").exists();
    let _ = std::fs::remove_dir_all(&tmp);
    let d = diags.iter().find(|d| d.code == "effect/multipleEffectProvide").unwrap_or_else(|| panic!("{diags:#?}"));
    assert_eq!(d.file.as_ref().unwrap().file_name().unwrap(), "main.efs");
    assert_eq!(d.line_col().map(|(l, _)| l), Some(2));
    assert!(!leftover, "the temporary tsconfig was not removed");
}

#[test]
fn run_on_imported_services_and_use_on_imported_effects_are_errors() {
    let root = root();
    let modules = root.join("examples/node_modules");
    if !modules.join("effect").exists() {
        return;
    }
    let tmp = std::env::temp_dir().join(format!("efs-e2e-bind-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(
        tmp.join("tsconfig.json"),
        std::fs::read_to_string(root.join("examples/tsconfig.json"))
            .unwrap()
            .replace("\"include\": [\"src\"]", "\"include\": [\".\"]"),
    )
    .unwrap();
    std::fs::copy(root.join("examples/package.json"), tmp.join("package.json")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&modules, tmp.join("node_modules")).unwrap();
    // A service written in plain TypeScript, and an effect exported from another EffScript file.
    std::fs::copy(root.join("examples/src/legacy/mailer.ts"), tmp.join("mailer.ts")).unwrap();
    std::fs::write(
        tmp.join("tasks.efs"),
        "import { Effect } from \"effect\"\n\nexport const cleanup = Effect.log(\"clean\")\n",
    )
    .unwrap();
    let src = "import { Clock, Effect } from \"effect\"
import { Mailer } from \"./mailer.js\"
import { cleanup } from \"./tasks.efs\"

export const bad = effect {
  const mailer = run Mailer
  const clock = run Clock.Clock
  use cleanup
  const nothing = use Effect.void
  return [mailer, clock, nothing]
}

export const good = effect {
  const mailer = use Mailer
  const clock = use Clock.Clock
  const now = run Clock.currentTimeMillis
  run cleanup
  run mailer.send(\"a@example.com\", \"hi\")
  return [clock, now]
}
";
    std::fs::write(tmp.join("main.efs"), src).unwrap();
    let mut session = efs_check::Session::new(efs_check::Workspace::discover(&tmp));
    let local = session.efs_diagnostics();
    let diags = session.type_diagnostics(true);
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(local.is_empty(), "{local:#?}");
    // `line:column` (both 1-based), code and the text of the reported span.
    let found: Vec<String> = diags
        .iter()
        .map(|d| {
            let span = d.span.unwrap();
            let (l, c) = d.line_col().unwrap();
            format!("{}:{} {} `{}`", l + 1, c + 1, d.code, &src[span.start as usize..span.end as usize])
        })
        .collect();
    assert_eq!(
        found,
        [
            "6:22 run-service `Mailer`",
            "7:21 run-service `Clock.Clock`",
            "8:7 use-non-service `cleanup`",
            "9:23 use-non-service `Effect.void`",
        ],
        "{diags:#?}"
    );
    assert_eq!(diags[0].message, "`Mailer` is a service; get it with `use Mailer`");
    assert_eq!(diags[2].message, "`cleanup` is an effect, not a service; run it with `run cleanup`");
    assert!(diags.iter().all(|d| d.file.as_ref().unwrap().ends_with("main.efs")));
}
