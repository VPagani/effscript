import type { CompileOptions } from "@effscript/compiler"

export type Options = Pick<CompileOptions, "importExtension" | "prelude">

/** Compiles `.efs` modules in `bun build` and the Bun runtime. */
export default function effscript(options?: Options): { name: string; setup(build: any): void }
