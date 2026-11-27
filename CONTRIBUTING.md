# Contributing to EffScript

Thanks for helping. This page covers setting up, testing, and how commits
are written.

## Setup

You need Rust 1.100 (pinned in `rust-toolchain.toml`), Node.js 22 and pnpm 10
(`corepack enable` installs the version pinned in `package.json`). Bun is
optional; the bundler tests use it when it is installed.

```bash
pnpm install
```

```bash
cargo test --workspace
```

Installing the JavaScript dependencies turns on the end-to-end tests, which type
check `examples/` and `tests/fixtures/` against the real `effect`,
`@typeonce/effect-machine` and `@effect/ui` packages.

Useful checks before you open a pull request (CI runs the same):

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo run -q -p efs_cli -- fmt --check examples tests/fixtures && cargo run -q -p efs_cli -- lint --deny-warnings examples tests/fixtures
```

The bundler plugins are tested by building a small app with each bundler and
running the output:

```bash
pnpm test:bundlers
```

When a change alters generated TypeScript or JavaScript on purpose, refresh the snapshots
and review the diff:

```bash
UPDATE_SNAPSHOTS=1 cargo test -p efs_codegen --test snapshots
```

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org/)
with a scope:

```
<type>(<scope>): <description>

[body: what changed and why]

[BREAKING CHANGE: what users must change]
```

- **Description**: imperative mood, lowercase, no final period, at most 72
  characters for the whole first line. "add", not "added" or "adds".
- **Body**: explain why the change is needed and anything a reviewer could
  not guess from the diff. Wrap at about 72 columns.
- **Breaking changes**: add `!` after the scope and a `BREAKING CHANGE:`
  footer. Until 1.0 the language changes often, and these markers are how
  users find out what to update.

| Type | Use for |
| --- | --- |
| `feat` | A new capability for users: syntax, a command, a lint rule. |
| `fix` | A bug fix. |
| `perf` | A speedup with no behavior change. |
| `refactor` | Restructuring with no behavior change. |
| `docs` | Documentation only. |
| `test` | Tests only. |
| `build`, `ci` | The build, packaging or CI. |
| `chore` | Anything else (renames, housekeeping). |
| `revert` | Reverting an earlier commit. |

| Scope | Area |
| --- | --- |
| `lang` | The language itself, when a change spans the parser, codegen and tooling. |
| `syntax` | Lexer and parser (`crates/efs_syntax`). |
| `codegen` | TypeScript generation (`crates/efs_codegen`). |
| `check` | Type checking with tsrs (`crates/efs_check`). |
| `fmt`, `lint`, `lsp`, `mapper`, `cli` | The crate of the same name. |
| `wasm` | The WebAssembly compiler (`crates/efs_wasm`, `@effscript/compiler`). |
| `plugins` | The bundler plugins (`packages/*`). |
| `editor` | The VS Code extension. |
| `npm` | The `effscript` npm package (the CLI launcher). |
| `examples`, `docs`, `ci`, `deps` | Examples, documentation, CI and dependency updates. |

Several scopes are separated by commas, `fix(syntax,fmt): ...`; leave the
scope out only for changes that touch nearly everything.

Examples:

```
feat(lint): add a prefer-const rule
fix(codegen): keep spans for imports merged into an existing import
feat(lang)!: use JavaScript's const and let
docs: describe component updates in the language guide
```

To check messages as you commit, enable the hook once:

```bash
git config core.hooksPath .githooks
```

Pull requests are checked in CI with `scripts/check-commit-msg.sh`.
