import type { CompileOptions } from "@effscript/compiler"

export type Options = Pick<CompileOptions, "importExtension" | "prelude">

export interface Plugin {
  name: string
  transform(this: unknown, code: string, id: string): { code: string; map: string | null } | null
}

/** Compiles `.efs` modules. Works with Rollup, Rolldown and Vite. */
export default function effscript(options?: Options): Plugin
