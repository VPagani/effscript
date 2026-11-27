//! The prelude: core `effect` modules used without an import get one.

use efs_codegen::{Options, SegmentKind, Target};

fn ts(src: &str) -> efs_codegen::Output {
    let out = efs_codegen::compile(src, &Options::default());
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    check_spans(src, &out);
    out
}

fn js(src: &str) -> efs_codegen::Output {
    let out = efs_codegen::compile(src, &Options { target: Target::JavaScript, ..Default::default() });
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    check_spans(src, &out);
    out
}

fn first_line(code: &str) -> &str {
    code.lines().next().unwrap_or_default()
}

fn check_spans(src: &str, out: &efs_codegen::Output) {
    let mut prev_end = 0;
    for s in &out.segments {
        assert!(s.gen_start >= prev_end, "overlapping segments at {}", s.gen_start);
        prev_end = s.gen_start + s.gen_len;
        assert!(prev_end as usize <= out.code.len());
        if s.kind == SegmentKind::Verbatim {
            assert_eq!(
                &out.code[s.gen_start as usize..prev_end as usize],
                &src[s.src_start as usize..(s.src_start + s.src_len) as usize]
            );
        }
    }
}

#[test]
fn imports_what_the_file_uses() {
    let src = "export const main = Effect.succeed(1) |> Effect.delay(Duration.seconds(1));\n";
    let out = ts(src);
    assert_eq!(first_line(&out.code), r#"import { pipe, Effect, Duration } from "effect""#);
    assert!(out.code.contains("\n\nexport const main = pipe(Effect.succeed(1)"), "{}", out.code);
    // Every reference to a prelude name is recorded, at its source position.
    let refs: Vec<&str> = out.prelude_refs.iter().map(|s| s.text(src)).collect();
    assert_eq!(refs, ["Effect", "Effect", "Duration"]);
    // The names still map back to the source.
    let at = src.find("Duration").unwrap() as u32;
    let g = out.to_generated(at).unwrap();
    assert_eq!(&out.code[g as usize..g as usize + 8], "Duration");
}

#[test]
fn nothing_is_imported_for_files_that_do_not_use_it() {
    let out = ts("export const n = 1;\n");
    assert_eq!(out.code, "export const n = 1\n");
}

#[test]
fn type_only_uses_are_imported_in_typescript() {
    let src = "export type Job = Effect<number>;\nexport type Items = Stream.Stream<string>;\n";
    let out = ts(src);
    assert_eq!(first_line(&out.code), r#"import { Effect, Stream } from "effect""#);
    assert!(out.code.contains("export type Job = Effect.Effect<number>"), "{}", out.code);
}

#[test]
fn type_only_uses_are_not_imported_in_javascript() {
    let src = "function run(job: Effect<number>, o: Option<string>): number {\n  return 1;\n}\nexport const x = run;\n";
    let out = js(src);
    assert!(!out.code.contains("import"), "{}", out.code);
    // A value use is.
    let out = js("export const later = (job: Option<number>) => Effect.delay(job, \"1 second\");\n");
    assert_eq!(first_line(&out.code), r#"import { Effect } from "effect""#);
}

#[test]
fn merges_into_an_existing_effect_import() {
    let src = "import { Data } from \"effect\";\n\nexport const main = Console.log(Data.struct({}));\n";
    let out = ts(src);
    assert_eq!(first_line(&out.code), r#"import { Data, Console } from "effect""#);
    assert_eq!(out.code.matches("from \"effect\"").count(), 1);
    // JavaScript too.
    let out = js(src);
    assert_eq!(first_line(&out.code), r#"import { Data, Console } from "effect""#);
}

#[test]
fn local_declarations_win() {
    let src = "service Clock { now: Effect<number> }\n\nconst Random = { next: () => 4 };\n\nexport const t = [Clock, Random.next()];\n";
    let out = ts(src);
    assert_eq!(first_line(&out.code), r#"import { Context, Effect } from "effect""#);
    assert!(!out.code.contains("Clock }") && !out.code.contains("Random }"), "{}", out.code);
}

#[test]
fn explicit_imports_win() {
    let src = "import { Effect } from \"./my-effect.js\";\nimport type { Option } from \"effect\";\n\nexport const main = Effect.succeed(1);\nexport type O = Option<number>;\n";
    let out = ts(src);
    assert_eq!(out.code.matches("import").count(), 2, "{}", out.code);
    assert!(out.code.starts_with("import { Effect } from \"./my-effect.js\"\nimport type { Option } from \"effect\""));
    assert!(out.prelude_refs.is_empty());
    // A module imported under another name is no reason to import the prelude's.
    let out = ts("import * as Fx from \"effect/Effect\";\n\nexport const main = Fx.succeed(1);\n");
    assert!(!out.code.contains("from \"effect\""), "{}", out.code);
}

#[test]
fn inner_bindings_shadow_the_prelude() {
    let src = "\
export function f(Schedule: number): number {
  return Schedule + 1;
}
export const g = (Option: string) => Option.length;
export function h() {
  const Console = { log: (s: string) => s };
  return Console.log(\"x\");
}
export const i = [1].map((x) => {
  for Ref of [x] { return Ref; }
  return x;
});
export function j<Fiber>(x: Fiber): Fiber {
  return x;
}
";
    let out = ts(src);
    assert!(!out.code.contains("import"), "{}", out.code);
    assert!(out.prelude_refs.is_empty());
    // A shadowed use does not hide another, unshadowed one.
    let out = ts("export const f = (Option: number) => Option;\nexport const g = Option.some(1);\n");
    assert_eq!(first_line(&out.code), r#"import { Option } from "effect""#);
    // Bindings in `catch` and `match` arms shadow too.
    let src = "\
error E { Cause: string }
export const c = Effect.fail(new E({ Cause: \"x\" })) catch { E({ Cause }) => Effect.succeed(Cause) };
export const m = (e: E) => match e { E({ Cause }) => Cause };
";
    let out = ts(src);
    assert_eq!(first_line(&out.code), r#"import { Data, Effect, Match } from "effect""#);
}

#[test]
fn hoisted_block_declarations_shadow_earlier_uses() {
    let src = "export function f() {\n  const a = Exit.x;\n  function Exit() {}\n  return a;\n}\n";
    let out = ts(src);
    assert!(!out.code.contains("import"), "{}", out.code);
}

#[test]
fn schema_fields_use_prelude_values() {
    let src = "export schema S { when: DateTime.Utc, n: Option<number> }\n";
    let out = js(src);
    assert_eq!(first_line(&out.code), r#"import { Schema, DateTime } from "effect""#);
}

#[test]
fn can_be_turned_off() {
    let src = "export const main = Effect.succeed(1);\nexport type T = Effect<number>;\n";
    let out = efs_codegen::compile(src, &Options { prelude: false, ..Default::default() });
    assert_eq!(out.code, "export const main = Effect.succeed(1)\nexport type T = Effect<number>\n");
    assert!(out.prelude_refs.is_empty());
}
