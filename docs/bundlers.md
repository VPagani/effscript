# Bundlers

The bundler plugins compile `.efs` files to JavaScript with source maps that
point back to the EffScript source. They run the compiler as WebAssembly
(`@effscript/compiler`), so they need no native binaries and work wherever the
bundler runs.

Import `.efs` files with their extension:

```js
import { program } from "./app.efs"
```

| Bundler | Package | Setup |
| --- | --- | --- |
| Vite | `@effscript/vite` | `plugins: [effscript()]` in `vite.config.ts` |
| Rollup | `@effscript/rollup` | `plugins: [effscript()]` |
| Rolldown | `@effscript/rolldown` | `plugins: [effscript()]` |
| esbuild | `@effscript/esbuild` | `plugins: [effscript()]` in `build({ ... })` |
| Bun | `@effscript/bun` | `plugins: [effscript()]` in `Bun.build({ ... })`, or `Bun.plugin(effscript())` in a preload script for the runtime |
| webpack, Rspack | `@effscript/webpack` | `module.rules: [{ test: /\.efs$/, use: "@effscript/webpack" }]` |

For example, with Vite:

```bash
pnpm add -D @effscript/vite
```

```ts
// vite.config.ts
import { defineConfig } from "vite"
import effscript from "@effscript/vite"

export default defineConfig({ plugins: [effscript()] })
```

The plugins take the compiler's `importExtension` and `prelude` options, for
example `effscript({ prelude: false })`.

Errors are reported by the bundler with their file, line and column. The
plugins do not type check: run `efs check` (or the [content
mapper](typescript.md)) for types.

## The compiler package

`@effscript/compiler` compiles from any JavaScript tool:

```js
import { compile } from "@effscript/compiler"

const { code, map, diagnostics } = compile(source, { filename: "app.efs", target: "js" })
```

| Option | Default | |
| --- | --- | --- |
| `filename` | `"input.efs"` | Used in source maps and diagnostics. |
| `target` | `"ts"` | `"js"` drops types. |
| `importExtension` | `".js"` | Replaces `.efs` in relative imports. |
| `prelude` | `true` | Imports the [prelude](language.md#prelude) modules a file uses. `false` turns it off. |
| `sourceMap` | `true` | Include a Source Map v3. |

`transform(source, file)` is what the plugins use: JavaScript output that
keeps `.efs` in imports, throwing `EffScriptError` when there are errors.

The package also formats and lints:

```js
import { format, lint } from "@effscript/compiler"

format(source).code // null when the source does not parse
lint(source, { disabled: ["prefer-const"] }).diagnostics
```

Each diagnostic has a one-based `line` and `column`, UTF-8 byte offsets
`start` and `end`, and UTF-16 offsets `from` and `to` for JavaScript strings.
`formatDiagnostic(file, d)` prints one as `file:line:column: severity: message [code]`.

### In the browser

`@effscript/compiler/browser` loads the same compiler with `fetch`, so it runs
in a web page; bundlers copy `efs.wasm` next to it. The
[website's playground](../website/src/lib/compiler.efs) uses it:

```js
import { init } from "@effscript/compiler/browser"

const efs = await init()
const { code } = efs.compile(source, { target: "js" })
```
