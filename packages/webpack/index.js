import { transform } from "@effscript/compiler"

/**
 * webpack and Rspack loader for `.efs` modules:
 * `{ test: /\.efs$/, use: "@effscript/webpack" }`.
 * @this {any}
 * @param {string} source
 */
export default function effscriptLoader(source) {
  const options = typeof this.getOptions === "function" ? this.getOptions() : {}
  try {
    const result = transform(source, this.resourcePath, options)
    for (const w of result.warnings) this.emitWarning(new Error(w.formatted))
    this.callback(null, result.code, result.map ? JSON.parse(result.map) : undefined)
  } catch (e) {
    this.callback(e)
  }
}
