//! `run` on a service and `use` on something that is not a service, when the operand is
//! declared in the same file.

const DECLS: &str = "import { Context, Effect, Layer } from \"effect\"
import { Mailer } from \"./mailer.ts\"

service Repo {
  all: Effect<number[]>
}
const Key = Context.Service<{ n: number }>(\"Key\")
layer RepoLive for Repo = { all: Effect.succeed([]) }
error Missing
const program = effect { return 1 }
effect function load() {
  return 1
}
const settings = { port: 1 }
";

fn compile(body: &str) -> efs_codegen::Output {
    let src = format!("{DECLS}export const main = effect {{\n{body}\n}}\n");
    efs_codegen::compile(&src, &Default::default())
}

fn errors(body: &str) -> Vec<(String, String, String)> {
    let src = format!("{DECLS}export const main = effect {{\n{body}\n}}\n");
    efs_codegen::compile(&src, &Default::default())
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.span.text(&src).to_string(), d.message))
        .collect()
}

#[test]
fn run_on_a_local_service_is_an_error() {
    let e = errors("  const repo = run Repo\n  const key = run Key");
    assert_eq!(e.len(), 2, "{e:?}");
    assert_eq!(e[0].0, "run-service");
    assert_eq!(e[0].1, "Repo");
    assert_eq!(e[0].2, "`Repo` is a service; get it with `use Repo`");
    assert_eq!((e[1].0.as_str(), e[1].1.as_str()), ("run-service", "Key"));
}

#[test]
fn use_on_a_local_non_service_is_an_error() {
    let e = errors(
        "  const a = use program\n  const b = use RepoLive\n  const c = use Missing\n  const d = use load\n  const s = use settings",
    );
    let found: Vec<(&str, &str)> = e.iter().map(|(c, s, _)| (c.as_str(), s.as_str())).collect();
    assert_eq!(
        found,
        vec![
            ("use-non-service", "program"),
            ("use-non-service", "RepoLive"),
            ("use-non-service", "Missing"),
            ("use-non-service", "load"),
            ("use-non-service", "settings"),
        ]
    );
    assert_eq!(e[0].2, "`program` is an effect, not a service; run it with `run program`");
    assert!(e[1].2.contains("is a layer, not a service; provide it with `with RepoLive`"), "{e:?}");
    assert!(e[3].2.contains("`run load(...)`"), "{e:?}");
}

#[test]
fn use_and_run_compile_to_yield_star() {
    let out =
        compile("  const repo = use Repo\n  const key = use Key\n  const n = run program\n  return (use Repo).all");
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(out.code.contains("const repo = yield* Repo\n"), "{}", out.code);
    assert!(out.code.contains("const key = yield* Key\n"), "{}", out.code);
    assert!(out.code.contains("const n = yield* program\n"), "{}", out.code);
    assert!(out.code.contains("return (yield* Repo).all\n"), "{}", out.code);
    // Settled from the declarations, so nothing is left for the type checker.
    assert!(out.binds.is_empty(), "{:?}", out.binds);
}

#[test]
fn other_operands_are_left_to_the_type_checker() {
    let body = "  const mailer = use Mailer\n  const x = run Mailer\n  const all = run Effect.succeed(1)";
    let src = format!("{DECLS}export const main = effect {{\n{body}\n}}\n");
    let out = efs_codegen::compile(&src, &Default::default());
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let sites: Vec<(efs_codegen::BindKind, &str, &str)> = out
        .binds
        .iter()
        .map(|b| (b.kind, b.operand.text(&src), &out.code[b.gen_pos as usize..b.gen_pos as usize + 6]))
        .collect();
    use efs_codegen::BindKind::{Run, Use};
    assert_eq!(sites, vec![(Use, "Mailer", "yield*"), (Run, "Mailer", "yield*"), (Run, "Effect.succeed(1)", "yield*")]);
}

#[test]
fn generated_positions_account_for_added_imports() {
    // `Layer` is not imported, so codegen adds it to the `effect` import above the sites.
    let src = "import { Effect } from \"effect\"\nimport { Mailer } from \"./mailer.ts\"\nservice S {\n  n: number\n}\nlayer L for S = { n: 1 }\nexport const main = effect {\n  const m = use Mailer\n}\n";
    let out = efs_codegen::compile(src, &Default::default());
    assert!(out.code.contains("import { Effect, Context, Layer }"), "{}", out.code);
    let b = out.binds[0];
    assert_eq!(&out.code[b.gen_pos as usize..b.gen_pos as usize + 13], "yield* Mailer");
    let src = "import { Mailer } from \"./mailer.ts\"\nexport const main = effect {\n  const m = use Mailer\n}\n";
    let out = efs_codegen::compile(src, &Default::default());
    assert!(out.code.starts_with("import { Effect } from \"effect\"\n"), "{}", out.code);
    let b = out.binds[0];
    assert_eq!(&out.code[b.gen_pos as usize..b.gen_pos as usize + 13], "yield* Mailer");
}

#[test]
fn shadowed_names_are_left_to_the_type_checker() {
    let e = errors(
        "  const f = effect (Repo: Effect<number>) => run Repo\n  const g = (program: typeof Repo) => effect { return use program }",
    );
    assert!(e.is_empty(), "{e:?}");
}
