import type { Options, Plugin } from "@effscript/rollup"

export type { Options }

/** Compiles `.efs` modules in Vite. */
export default function effscript(options?: Options): Plugin & { enforce: "pre" }
