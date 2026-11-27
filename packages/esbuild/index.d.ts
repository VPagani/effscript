import type { CompileOptions } from "@effscript/compiler"

export type Options = Pick<CompileOptions, "importExtension" | "prelude">

/** Compiles `.efs` modules in esbuild. */
export default function effscript(options?: Options): { name: string; setup(build: any): void }
