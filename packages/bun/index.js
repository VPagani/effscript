import { readFile } from "node:fs/promises"
import { filter, transform, withInlineMap } from "@effscript/compiler"

/**
 * Compiles `.efs` modules in `bun build` (`Bun.build({ plugins: [effscript()] })`) and in the
 * Bun runtime (`Bun.plugin(effscript())`, for example from a preload script).
 * @param {import("./index.d.ts").Options} [options]
 */
export default function effscript(options = {}) {
  return {
    name: "effscript",
    setup(build) {
      build.onLoad({ filter }, async (args) => {
        const source = await readFile(args.path, "utf8")
        const result = transform(source, args.path, options)
        for (const w of result.warnings) console.warn(w.formatted)
        return { contents: withInlineMap(result.code, result.map), loader: "js" }
      })
    },
  }
}
