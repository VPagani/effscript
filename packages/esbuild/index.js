import { readFile } from "node:fs/promises"
import { EffScriptError, filter, transform, withInlineMap } from "@effscript/compiler"

const location = (file, d) => ({ file, line: d.line, column: d.column - 1 })

/**
 * Compiles `.efs` modules in esbuild.
 * @param {import("./index.d.ts").Options} [options]
 */
export default function effscript(options = {}) {
  return {
    name: "effscript",
    setup(build) {
      build.onLoad({ filter }, async (args) => {
        const source = await readFile(args.path, "utf8")
        try {
          const result = transform(source, args.path, options)
          return {
            contents: withInlineMap(result.code, result.map),
            loader: "js",
            warnings: result.warnings.map((w) => ({ text: w.message, location: location(args.path, w) })),
          }
        } catch (e) {
          if (!(e instanceof EffScriptError)) throw e
          return { errors: e.diagnostics.map((d) => ({ text: d.message, location: location(args.path, d) })) }
        }
      })
    },
  }
}
