// The EffScript compiler, running as WebAssembly.
import { readFileSync } from "node:fs"

/** @type {any} */
let wasm

function instance() {
  if (!wasm) {
    const bytes = readFileSync(new URL("./efs.wasm", import.meta.url))
    wasm = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports
  }
  return wasm
}

const encoder = new TextEncoder()
const decoder = new TextDecoder()

function write(w, text) {
  const bytes = encoder.encode(text)
  const ptr = w.efs_alloc(bytes.length)
  new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
  return [ptr, bytes.length]
}

/**
 * Compiles EffScript source.
 * @param {string} source
 * @param {import("./index.d.ts").CompileOptions} [options]
 * @returns {import("./index.d.ts").CompileResult}
 */
export function compile(source, options = {}) {
  const w = instance()
  const [src, srcLen] = write(w, source)
  const [opts, optsLen] = write(w, JSON.stringify(options))
  const packed = BigInt.asUintN(64, w.efs_compile(src, srcLen, opts, optsLen))
  const ptr = Number(packed >> 32n)
  const len = Number(packed & 0xffffffffn)
  const json = decoder.decode(new Uint8Array(w.memory.buffer, ptr, len).slice())
  w.efs_free(ptr, len)
  w.efs_free(src, srcLen)
  w.efs_free(opts, optsLen)
  return JSON.parse(json)
}

/** Matches the files bundler plugins handle. */
export const filter = /\.efs$/

/** A compile error, with the diagnostics that caused it. */
export class EffScriptError extends Error {
  /** @param {string} file @param {import("./index.d.ts").Diagnostic[]} diagnostics */
  constructor(file, diagnostics) {
    super(diagnostics.map((d) => format(file, d)).join("\n"))
    this.name = "EffScriptError"
    this.file = file
    this.diagnostics = diagnostics
  }
}

/** `file:line:column: message [code]` */
export function format(file, d) {
  return `${file}:${d.line}:${d.column}: ${d.severity}: ${d.message} [${d.code}]`
}

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
    warnings: result.diagnostics.filter((d) => d.severity !== "error").map((d) => ({ ...d, formatted: format(file, d) })),
  }
}

/** `code` followed by its source map, inline. */
export function withInlineMap(code, map) {
  if (!map) return code
  return `${code}\n//# sourceMappingURL=data:application/json;base64,${Buffer.from(map).toString("base64")}\n`
}
