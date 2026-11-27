import rollupPlugin from "@effscript/rollup"

/**
 * Compiles `.efs` modules in Vite, before other transforms see them.
 * @param {import("./index.d.ts").Options} [options]
 */
export default function effscript(options = {}) {
  return { ...rollupPlugin(options), enforce: "pre" }
}
