# Getting started

## Install

EffScript is not on npm yet. Build the `efs` command from this repository
(Rust 1.100 or newer):

```bash
cargo install --path crates/efs_cli
```

## Create a project

```bash
efs init hello && cd hello
```

```bash
pnpm install
```

This creates a TypeScript project with `src/main.efs`, a `tsconfig.json` and
an `efs.json`. Check it, build it and run it:

```bash
efs check && efs build && pnpm tsc && node dist/main.js
```

`efs build` writes `src/main.ts` next to the source, and TypeScript compiles it
with the rest of your code. To skip TypeScript and write JavaScript directly,
use `efs build --emit js`, or use a [bundler plugin](bundlers.md).

## A first program

```efs
import { Console } from "effect"

error EmptyName

effect function greet(name: string) {
  if name.length == 0 {
    fail new EmptyName()
  }
  run Console.log(`Hello, ${name}!`)
}

export const main = greet("Effect")
  catch {
    EmptyName => Console.log("Who are you?")
  }
```

`effect function` and `run` work like `async function` and `await`: `run`
runs an effect and gives you its result. `use Service` gets a service your
code depends on. See the [language guide](language.md)
for everything else, and [what each construct compiles to](language.md).

## Next steps

- [The `efs` command](cli.md): building, checking, formatting and linting.
- [Editors](editors.md): the VS Code extension and the language server.
- [TypeScript interop](typescript.md): using EffScript in a TypeScript project.
- [Bundlers](bundlers.md): Vite, Rollup, Rolldown, esbuild, Bun and webpack.
