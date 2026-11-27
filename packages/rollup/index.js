import { EffScriptError, filter, transform } from "@effscript/compiler"

/** Queries that ask the bundler for something other than the compiled module, such as `?raw`. */
const skipped = /(^|&)(raw|url|inline)(=|&|$)/

/**
 * Compiles `.efs` modules. Works with Rollup, Rolldown and Vite, which share this plugin API.
 * @param {import("./index.d.ts").Options} [options]
 */
export default function effscript(options = {}) {
  return {
    name: "effscript",
    transform(code, id) {
      const [file, query = ""] = id.split("?")
      if (!filter.test(file) || skipped.test(query)) return null
      try {
        const result = transform(code, file, options)
        for (const w of result.warnings) this.warn(w.formatted)
        return { code: result.code, map: result.map }
      } catch (e) {
        if (e instanceof EffScriptError) this.error(e.message)
        throw e
      }
    },
  }
}
