export interface CompileOptions {
  /** The file name, used in source maps and diagnostics. */
  filename?: string
  /** `"js"` drops types; `"ts"` keeps them. Default `"ts"` for `compile`, `"js"` for `transform`. */
  target?: "js" | "ts"
  /** Replaces `.efs` in relative imports. Default `".js"` for `compile`, `".efs"` for `transform`. */
  importExtension?: string
  /** Import the core Effect modules a file uses (`Effect`, `Schema`, ...) without an explicit import. Default `true`. */
  prelude?: boolean
  /** Include a Source Map v3. Default `true`. */
  sourceMap?: boolean
}

export interface Diagnostic {
  message: string
  /** A stable identifier such as `syntax` or `codegen`. */
  code: string
  severity: "error" | "warning" | "info"
  /** Byte offsets in the source. */
  start: number
  end: number
  /** One-based. */
  line: number
  /** One-based, in UTF-16 code units. */
  column: number
}

export interface CompileResult {
  code: string
  /** A Source Map v3 JSON string, or `null` when `sourceMap` is false. */
  map: string | null
  diagnostics: Diagnostic[]
}

export declare function compile(source: string, options?: CompileOptions): CompileResult

export interface TransformResult {
  code: string
  map: string | null
  warnings: Array<Diagnostic & { formatted: string }>
}

/** Compiles for a bundler; throws `EffScriptError` when there are errors. */
export declare function transform(source: string, file: string, options?: CompileOptions): TransformResult

export declare const filter: RegExp

export declare class EffScriptError extends Error {
  readonly file: string
  readonly diagnostics: Diagnostic[]
  constructor(file: string, diagnostics: Diagnostic[])
}

export declare function format(file: string, diagnostic: Diagnostic): string
export declare function withInlineMap(code: string, map: string | null): string
