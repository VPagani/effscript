# TypeScript interop

EffScript compiles to TypeScript, so an EffScript project is a TypeScript
project: the same `tsconfig.json`, the same packages and the same checker.

## Importing in both directions

EffScript and TypeScript modules import each other in both directions.
Code written in plain TypeScript (services, layers, errors, schemas, effects)
is used from EffScript exactly as it is, with its real types:

```ts
// legacy/mailer.ts: plain TypeScript
export class MailError extends Data.TaggedError("MailError")<{ readonly reason: string }> {}

export class Mailer extends Context.Service<Mailer, {
  readonly send: (to: string, body: string) => Effect.Effect<void, MailError>
}>()("Mailer") {}

export const MailerConsole = Layer.succeed(Mailer, { send: (to, body) => Effect.log(`${to}: ${body}`) })
```

```efs
// onboarding.efs
import { Mailer, MailerConsole } from "./legacy/mailer.js"

export effect function onboard(to: string) {
  const mailer = run Mailer
  run mailer.send(to, "Welcome!")
}

export const onboarding = onboard("ada@example.com")
  catch {
    MailError(e) => Effect.logWarning(e.reason)
  }
  with MailerConsole
```

```ts
// report.ts: plain TypeScript again
import { onboarding } from "./onboarding.js"
```

An `.efs` file and a hand-written `.ts` file with the same name would
collide, because the build writes `name.ts`. `efs build` refuses to overwrite
a `.ts` file it did not generate, and `efs check` reports the conflict.

EffScript modules import and export like TypeScript modules. `import ... from
"./x.efs"` becomes `./x.js` by default (set `importExtension` in `efs.json`),
which TypeScript resolves to the generated `x.ts`.

## Type checking

There are two ways to type check:

1. **`efs check`** runs [tsrs](https://github.com/oxc-project/tsrs), the Rust
   port of the TypeScript 7 checker, in process. Every `.efs` file is compiled
   in memory and overlaid on the file system as the `.ts` file `efs build`
   would write, so TypeScript files can import EffScript modules and the other way
   round. Errors are reported at their EffScript source position.
2. **TypeScript content mappers** (TypeScript 7.1): list the EffScript package in
   `tsconfig.json` and the TypeScript compiler and language server read `.efs`
   files directly, through `efs mapper`:

   ```json
   {
     "contentMappers": [
       { "package": "effscript", "extensions": [".efs"], "options": { "importExtension": ".efs" } }
     ]
   }
   ```

   Run `tsgo` or `tsrs` with `--runExternalCode`. To emit JavaScript through
   `tsc` this way, also enable `rewriteRelativeImportExtensions`.

## Effect Language Service

[effect-tsgo](https://github.com/Effect-TS/tsgo) (`@effect/tsgo`) is the
Effect team's language service for TypeScript: about 150 Effect-specific
diagnostics, with quick fixes and refactors. EffScript generates code that
passes it, and works with it in two ways:

- `efs check --effect` runs the project's `effect-tsgo` over the build output
  (written next to each `.efs` file, as `efs build` does) and reports each
  Effect diagnostic at its EffScript position. It enables the
  `@effect/language-service` plugin for the run if your tsconfig does not.

  ```bash
  pnpm add -D @effect/tsgo tsrs
  ```

  ```bash
  efs check --effect
  ```

- With the content mapper, run your editor or CLI through `effect-tsgo` (for
  example `effect-tsgo tsrs --noEmit --runExternalCode`) and its
  diagnostics apply to `.efs` files directly, like TypeScript's own.

`efs lint` has its own versions of the rules that fit EffScript's syntax, so
the most common ones show up as you type without effect-tsgo.

