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

export interface FormatOptions {
  /** Print `;` after statements. Default `true`. */
  semicolons?: boolean
}

export interface LintOptions {
  /** Rules to skip, such as `"prefer-const"`. */
  disabled?: string[]
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
  /** UTF-16 offsets in the source, for JavaScript strings. */
  from: number
  to: number
}

export interface CompileResult {
  code: string
  /** A Source Map v3 JSON string, or `null` when `sourceMap` is false. */
  map: string | null
  diagnostics: Diagnostic[]
}

export interface FormatResult {
  /** The formatted source, or `null` when it does not parse. */
  code: string | null
  diagnostics: Diagnostic[]
}

export interface LintResult {
  diagnostics: Diagnostic[]
}

export interface Compiler {
  compile(source: string, options?: CompileOptions): CompileResult
  format(source: string, options?: FormatOptions): FormatResult
  lint(source: string, options?: LintOptions): LintResult
}

export declare function compile(source: string, options?: CompileOptions): CompileResult
export declare function format(source: string, options?: FormatOptions): FormatResult
export declare function lint(source: string, options?: LintOptions): LintResult

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

/** `file:line:column: severity: message [code]` */
export declare function formatDiagnostic(file: string, diagnostic: Diagnostic): string
export declare function withInlineMap(code: string, map: string | null): string
