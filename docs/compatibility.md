# Library versions

EffScript generates calls into Effect and its companion libraries, so each
EffScript release is written for specific versions of them.

| Library | Supported |
| --- | --- |
| `effect` | 4.x |
| `@typeonce/effect-machine` | 0.47.x |
| `@effect/ui` | the preview build in `examples/package.json` |
| TypeScript (for `efs mapper`) | 7.1 |

`efs check` warns when an installed `effect` or `@typeonce/effect-machine` is
outside these ranges (`efs(unsupported-version)`), so a mismatch shows up as
one clear warning rather than as type errors in generated code.

## How EffScript keeps up

**Source stays stable; output follows Effect.** EffScript syntax names
concepts (`effect function`, `run`, `use`, `service`, `layer`, `error`,
`catch`, `with`) rather than Effect's function names, so most Effect API
changes are absorbed by the code generator and need no change to `.efs`
files. When Effect renames `Context.Tag` to `Context.Service`, or
`catchTags` changes shape, the next EffScript release generates the new
form.

**Minor versions.** Effect follows semver, so a new 4.x minor should keep
working. A weekly CI job (`compat` in `.github/workflows/`) type checks the
examples and fixtures, runs effect-tsgo and builds with every bundler plugin
against the newest `effect` 4.x and `@typeonce/effect-machine`, so a break is
found before users find it. When new APIs give a better translation (for
example a dedicated combinator for something EffScript builds by hand), the
code generator adopts it and the supported minimum is raised in this table.

**Major versions.** When Effect 5 enters beta, EffScript:

1. adds a target version to the code generator, chosen from the installed
   `effect` (or `"effect": "5"` in `efs.json`), with the differences kept in
   one place rather than scattered through the lowering code;
2. supports both majors for the transition, testing each in CI;
3. drops the old major in a later EffScript major release, after Effect's own
   support for it winds down.

New Effect features that need syntax (as `Effect.fn` needed
`effect function`) are added to the language in a minor EffScript release.
Syntax is only removed in an EffScript major, with an error that names the
replacement, as with `initial state`.

**Pre-1.0 libraries.** `@typeonce/effect-machine` and Effect UI change faster
than Effect itself. Their syntax in EffScript (`machine`, `component`) is
marked experimental until they reach 1.0, and an EffScript release may track
a breaking change in them.
