// The EffScript compiler, running as WebAssembly in Node.js, Bun or Deno.
import { readFileSync } from "node:fs"
import { compiler, EffScriptError, formatDiagnostic } from "./core.js"

export { EffScriptError, formatDiagnostic }

/** @type {any} */
let wasm

const efs = compiler(() => {
  if (!wasm) {
    const bytes = readFileSync(new URL("./efs.wasm", import.meta.url))
    wasm = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports
  }
  return wasm
})

/** Compiles EffScript source. */
export const compile = efs.compile

/** Formats EffScript source; `code` is `null` when it does not parse. */
export const format = efs.format

/** Runs the lint rules. */
export const lint = efs.lint

/** Matches the files bundler plugins handle. */
export const filter = /\.efs$/

/**
 * Compiles a file for a bundler: JavaScript with a source map, keeping `.efs` in imports so
 * that the bundler resolves them to the source files. Throws `EffScriptError` on errors.
 * @param {string} source
 * @param {string} file
 * @param {import("./index.d.ts").CompileOptions} [options]
 */
export function transform(source, file, options = {}) {
  const result = compile(source, { target: "js", importExtension: ".efs", sourceMap: true, ...options, filename: file })
  const errors = result.diagnostics.filter((d) => d.severity === "error")
  if (errors.length > 0) throw new EffScriptError(file, errors)
  return {
    code: result.code,
    map: result.map ?? null,
    warnings: result.diagnostics
      .filter((d) => d.severity !== "error")
      .map((d) => ({ ...d, formatted: formatDiagnostic(file, d) })),
  }
}

/** `code` followed by its source map, inline. */
export function withInlineMap(code, map) {
  if (!map) return code
  return `${code}\n//# sourceMappingURL=data:application/json;base64,${Buffer.from(map).toString("base64")}\n`
}
