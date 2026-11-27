// The compiler API over an instantiated efs.wasm, shared by the Node.js and browser entries.

const encoder = new TextEncoder()
const decoder = new TextDecoder()

/** Adds UTF-16 `from`/`to` offsets to diagnostics whose `start`/`end` are UTF-8 byte offsets. */
function withOffsets(source, diagnostics) {
  if (diagnostics.length === 0) return diagnostics
  const bytes = encoder.encode(source)
  const at = (byte) => decoder.decode(bytes.subarray(0, Math.min(byte, bytes.length))).length
  return diagnostics.map((d) => ({ ...d, from: at(d.start), to: at(d.end) }))
}

/**
 * The compiler functions for a WebAssembly instance's exports.
 * @param {() => any} exports
 */
export function compiler(exports) {
  function call(fn, source, options) {
    const w = exports()
    const write = (text) => {
      const bytes = encoder.encode(text)
      const ptr = w.efs_alloc(bytes.length)
      new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
      return [ptr, bytes.length]
    }
    const [src, srcLen] = write(source)
    const [opts, optsLen] = write(JSON.stringify(options))
    const packed = BigInt.asUintN(64, w[fn](src, srcLen, opts, optsLen))
    const ptr = Number(packed >> 32n)
    const len = Number(packed & 0xffffffffn)
    const json = decoder.decode(new Uint8Array(w.memory.buffer, ptr, len).slice())
    w.efs_free(ptr, len)
    w.efs_free(src, srcLen)
    w.efs_free(opts, optsLen)
    const result = JSON.parse(json)
    result.diagnostics = withOffsets(source, result.diagnostics)
    return result
  }
  return {
    /** @param {string} source @param {import("./index.d.ts").CompileOptions} [options] */
    compile: (source, options = {}) => call("efs_compile", source, options),
    /** @param {string} source @param {import("./index.d.ts").FormatOptions} [options] */
    format: (source, options = {}) => call("efs_format", source, options),
    /** @param {string} source @param {import("./index.d.ts").LintOptions} [options] */
    lint: (source, options = {}) => call("efs_lint", source, options),
  }
}

/** `file:line:column: severity: message [code]` */
export function formatDiagnostic(file, d) {
  return `${file}:${d.line}:${d.column}: ${d.severity}: ${d.message} [${d.code}]`
}

/** A compile error, with the diagnostics that caused it. */
export class EffScriptError extends Error {
  /** @param {string} file @param {import("./index.d.ts").Diagnostic[]} diagnostics */
  constructor(file, diagnostics) {
    super(diagnostics.map((d) => formatDiagnostic(file, d)).join("\n"))
    this.name = "EffScriptError"
    this.file = file
    this.diagnostics = diagnostics
  }
}
