# EffScript

**A readable syntax for [Effect](https://effect.website/).** EffScript is to Effect what
JSX was to React: a small language that compiles to the TypeScript you would
write by hand, minus the incantations. If you know `async function` and
`await`, you already know `effect function` and `run`.

```efs
import { Effect, Schedule } from "effect"

error UserNotFound { id: UserId }

service UserRepo {
  find(id: UserId): Effect<User, UserNotFound>
}

export effect function greet(id: UserId) {
  const repo = run UserRepo
  const user = run repo.find(id)
    |> Effect.retry(Schedule.recurs(2))
    |> Effect.timeout("1 second")
  run Effect.log(`Hello, ${user.name}`)
  return user
}

export const main = greet(UserId.make("u-1"))
  catch {
    UserNotFound(e) => Effect.log(`no user ${e.id}`)
    TimeoutError => Effect.log("too slow")
  }
  with UserRepoLive
```

compiles to

```ts
import { Effect, Schedule, Data, Context, pipe } from "effect"

class UserNotFound extends Data.TaggedError("UserNotFound")<{
  readonly id: UserId
}> {}

class UserRepo extends Context.Service<UserRepo, {
  readonly find: (id: UserId) => Effect.Effect<User, UserNotFound>
}>()("UserRepo") {}

export const greet = Effect.fn("greet")(function* (id: UserId) {
  const repo = yield* UserRepo
  const user = yield* pipe(repo.find(id), Effect.retry(Schedule.recurs(2)), Effect.timeout("1 second"))
  yield* Effect.log(`Hello, ${user.name}`)
  return user
})

export const main = Effect.provide(Effect.catchTags(greet(UserId.make("u-1")), {
  UserNotFound: (e) => Effect.log(`no user ${e.id}`),
  TimeoutError: () => Effect.log("too slow")
}), UserRepoLive)
```

EffScript covers Effect itself (generators, pipelines, services, layers, tagged
errors, schemas, `Match`, streams, scopes) as well as
[`@typeonce/effect-machine`](https://github.com/typeonce-dev/effect-machine)
state machines and [Effect UI](https://ui.effect.website/) components with
JSX-like markup. Read the **[language guide](docs/language.md)** for every
construct and what it compiles to.

## Why

Effect code is precise but noisy: `Effect.gen(function* () { ... })`,
`yield*` on every line, `Context.Service<Self, Shape>()("Key")`,
`Data.TaggedError("Tag")<{ ... }>`, schema combinators that restate your
types, statechart objects nested five levels deep. EffScript gives each of these a
name and a shape:

| Effect TypeScript | EffScript |
| --- | --- |
| `Effect.fn("f")(function* (a) { const x = yield* e })` | `effect function f(a) { const x = run e }` |
| `return yield* Effect.fail(err)` | `fail err` |
| `pipe(a, f, g)` / `Effect.provide(e, layer)` | `a \|> f \|> g` / `e with layer` |
| `Effect.catchTags(e, { A: (x) => h })` | `e catch { A(x) => h }` |
| `class S extends Context.Service<S, {...}>()("S") {}` | `service S { ... }` |
| `Layer.effect(S, Effect.gen(...))` | `layer SLive for S { ... }` |
| `class E extends Data.TaggedError("E")<{...}> {}` | `error E { ... }` |
| `Schema.Struct({ a: Schema.NullOr(Schema.String) })` | `schema S { a: String \| null }` |
| `Match.valueTags(x, { A: (a) => ... })` | `match x { A(a) => ... }` |
| `Machine.state(...)`, `Machine.make(...).handle(...)` | `machine M { state A { on E -> B } }` |
| `Html.component(...).make({ update: { A: ($) => $.evolve(...) } })` | `component C { let n: Number = 0; update A { n += 1 } }` |
| `h.div.class("x").children(...)` | `<div class="x">...</div>` |
| `Effect.Effect<A, E, R>` | `Effect<A, E, R>` |

Generated code is ordinary TypeScript: no runtime, no lock-in, and you can
read every line of it with `efs emit`.

## Type checking with tsrs

The compiler is written in Rust and type checks with
[tsrs](https://github.com/oxc-project/tsrs), the Rust port of the TypeScript 7
checker, linked in as a library:

- `efs check` compiles every `.efs` file in memory, lays the results over the
  file system where `efs build` would write them, builds a tsrs `Program` from
  your `tsconfig.json`, and maps every TypeScript error back to the EffScript line
  and column through a span map. TypeScript files and EffScript files import each
  other freely: existing Effect code in plain TypeScript (services, layers, errors,
  schemas) is used from EffScript with its real types, and the other way round.
- `efs mapper` speaks the TypeScript 7.1 **content mapper** protocol, so
  `tsgo`, `tsrs` and their language servers can read `.efs` files directly,
  with TypeScript's own diagnostics, hovers and completions in EffScript source.

```
$ efs check
src/users.efs:37:30: error: Argument of type 'number' is not assignable to parameter of type 'string & Brand<"UserId">'. [ts2345]
 37 |   const user = run repo.find(42)
    |                              ^^
checked 4 EffScript files in 95ms, 1 error
```

## Getting started

Build the CLI (Rust 1.100 or newer):

```bash
cargo install --path crates/efs_cli
```

Create a project and run it:

```bash
efs init hello && cd hello
```

```bash
npm install && efs check && efs build && npx tsc && node dist/main.js
```

## The `efs` command

| Command | What it does |
| --- | --- |
| `efs build [paths] [--out-dir D] [--source-map] [--watch]` | Compile `.efs` files to `.ts` (next to the source by default), with optional source maps. |
| `efs check [path] [--efs-only] [--json]` | Type check the project with tsrs and report errors in EffScript coordinates. |
| `efs fmt [paths] [--check] [--stdin]` | Format files. Idempotent, keeps comments, and never changes the compiled output. |
| `efs lint [paths] [--deny-warnings] [--disable RULE] [--rules]` | Effect-aware lint rules (see below). |
| `efs emit FILE [--span-map \| --source-map]` | Print the generated TypeScript, its span map or a source map. |
| `efs type-at FILE:LINE:COL` | Print the inferred type at a position. |
| `efs lsp` | Language server: diagnostics, hover types, formatting, symbols, go to definition. |
| `efs mapper` | TypeScript 7.1 content mapper over stdio. |
| `efs init [dir]` | Scaffold a project. |

Optional `efs.json` at the project root:

```json
{ "importExtension": ".js", "outDir": "gen", "sourceMap": true, "lint": { "disabled": ["unused-binding"] } }
```

### Lint rules

| Rule | Default | |
| --- | --- | --- |
| `discarded-effect` | warning | An effect built as a statement in an effect body is never run (`Console.log(x)` without `run`). |
| `throw-in-effect` | warning | `throw` in an effect body becomes a defect; use `fail`. |
| `prefer-fail` | hint | `return run Effect.fail(e)` is `fail e`. |
| `unused-binding` | warning | A local binding is never used. |
| `prefer-const` | warning | A `let` that is never reassigned should be `const`. |
| `pascal-case` | warning | Services, errors, schemas, machines, components, states, events and messages are PascalCase. |
| `unknown-event` | error | A machine handles an event it does not declare. |
| `unreachable-state` | warning | No transition enters a machine state. |
| `message-naming` | warning | Effect UI messages are past-tense facts (`ClickedSave`), not commands (`SetName`). |
| `strict-equality` | hint | `==` already compiles to `===`. |

Silence a rule for one line with `// efs-ignore: rule-name` on the line above.

## Editors

`editors/vscode` contains a VS Code extension (syntax highlighting, and the
language server with a "Show Generated TypeScript" command). Any editor with
LSP support can run `efs lsp`. For full TypeScript intelligence inside `.efs`
files, also enable the content mapper in `tsconfig.json` (see the
[guide](docs/language.md#interop-with-typescript)).

## npm

`npm/effscript` is the npm package (`effscript`): a small launcher for the native binary that also
declares `typescript.contentMapper`, so it can be listed in `contentMappers`
directly. Platform binaries are expected as `effscript-<platform>-<arch>`
packages, or point `EFS_BINARY` at a binary.

## Repository layout

| Crate | |
| --- | --- |
| `efs_syntax` | Lexer, parser and syntax tree. Newline-sensitive, with lazy re-lexing for markup text. |
| `efs_codegen` | Lowering to TypeScript with a span map (verbatim and atom segments, compatible with TypeScript's content-mapper format) and source maps. |
| `efs_check` | In-process type checking and hovers with tsrs. |
| `efs_fmt` | The formatter. |
| `efs_lint` | Lint rules. |
| `efs_lsp` | The language server. |
| `efs_mapper` | The TypeScript content-mapper server. |
| `efs_cli` | The `efs` binary. |

Tests: `cargo test --workspace`. Codegen snapshots live in `tests/snapshots`
(update with `UPDATE_SNAPSHOTS=1`). The end-to-end tests type check
`examples/` and `tests/fixtures/` against the real `effect`,
`@typeonce/effect-machine` and `@effect/ui` packages once you run
`npm install` in `examples/`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, tests and the commit
message convention (Conventional Commits with scopes).

## Status

EffScript is an early release that targets Effect 4.1, `@typeonce/effect-machine`
0.47 and the Effect UI preview. Effect UI and effect-machine are both pre-1.0,
so their syntax in EffScript may change with them. See
[Not supported](docs/language.md#not-supported) for current gaps.

## License

MIT
