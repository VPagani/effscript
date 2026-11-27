import type { Compiler } from "./index.d.ts"

export type {
  CompileOptions,
  CompileResult,
  Compiler,
  Diagnostic,
  FormatOptions,
  FormatResult,
  LintOptions,
  LintResult,
} from "./index.d.ts"
export { EffScriptError, formatDiagnostic } from "./index.d.ts"

/**
 * Loads the compiler. By default `efs.wasm` is fetched from next to this module; pass a URL,
 * a `Response` or the bytes to load it from elsewhere.
 */
export declare function init(input?: string | URL | Response | BufferSource): Promise<Compiler>
