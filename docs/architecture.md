# Architecture

The compiler is written in Rust and type checks with
[tsrs](https://github.com/oxc-project/tsrs), the Rust port of the TypeScript 7
checker, linked in as a library.

```
.efs ──parse──▶ syntax tree ──lower──▶ TypeScript or JavaScript + span map
                                            │
                     efs check: overlay on the file system ──▶ tsrs Program
                     efs mapper: TypeScript's content-mapper protocol
                     @effscript/compiler: WebAssembly, for bundler plugins
```

Every piece of generated text is recorded in a **span map**: a verbatim
segment where the output copies the source, or an atom segment that maps a
whole generated construct to the source it came from. The span map uses
TypeScript's content-mapper format, and it is how diagnostics, hovers and
source maps point back to EffScript positions.

| Crate | |
| --- | --- |
| `efs_syntax` | Lexer, parser and syntax tree. Newline-sensitive, with re-lexing for markup text. |
| `efs_codegen` | Lowering to TypeScript or JavaScript, the span map, and source maps. |
| `efs_check` | In-process type checking, hovers and declaration emit with tsrs, and the effect-tsgo bridge. |
| `efs_fmt` | The formatter. |
| `efs_lint` | Lint rules. |
| `efs_lsp` | The language server. |
| `efs_mapper` | The TypeScript content-mapper server. |
| `efs_wasm` | The code generator as WebAssembly. |
| `efs_cli` | The `efs` command. |

| Package | |
| --- | --- |
| `packages/effscript` | `effscript`: a launcher for the native `efs` binary that also declares the content mapper. |
| `packages/compiler` | `@effscript/compiler`: the WebAssembly compiler for JavaScript tools. |
| `packages/{vite,rollup,rolldown,esbuild,bun,webpack}` | The bundler plugins. |
| `packages/bundler-tests` | Builds a small app with every plugin and runs it. |
| `editors/vscode` | The VS Code extension. |

## Tests

- `tests/snapshots` holds the expected TypeScript and JavaScript for every
  example and fixture (`UPDATE_SNAPSHOTS=1` accepts changes).
- The end-to-end tests type check `examples/` and `tests/fixtures/` against
  the real `effect`, `@typeonce/effect-machine` and `@effect/ui` packages, and
  run effect-tsgo, once `pnpm install` has run.
- `pnpm test:bundlers` builds and runs a small app with every bundler plugin.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the development workflow.
