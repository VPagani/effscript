import { EffScriptError, filter, transform } from "@effscript/compiler"

/**
 * Compiles `.efs` modules. Works with Rollup, Rolldown and Vite, which share this plugin API.
 * @param {import("./index.d.ts").Options} [options]
 */
export default function effscript(options = {}) {
  return {
    name: "effscript",
    transform(code, id) {
      const file = id.split("?")[0]
      if (!filter.test(file)) return null
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
