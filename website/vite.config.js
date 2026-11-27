import tailwindcss from "@tailwindcss/vite"
import effscript from "@effscript/vite"
import { plugin as prerender } from "@effect/ui/html/HtmlServerVite"
import { markdown } from "@effect/ui/html/MarkdownVite"
import { defineConfig } from "vite"

/**
 * The prerender runs the pages in Node, where the WebAssembly compiler has nothing to load
 * from: there it never finishes loading, and the pages render what they show until it has.
 */
const compilerOnlyInTheBrowser = () => ({
  name: "compiler-only-in-the-browser",
  enforce: "pre",
  applyToEnvironment: (environment) => environment.name === "ssr",
  resolveId: (source) => (source === "@effscript/compiler/browser" ? "\0compiler-stub" : undefined),
  load: (id) =>
    id === "\0compiler-stub"
      ? [
          "export const init = () => new Promise(() => {})",
          "export class EffScriptError extends Error {}",
          "export const formatDiagnostic = (d) => d.message",
        ].join("\n")
      : undefined,
})

export default defineConfig({
  plugins: [
    effscript(),
    // The documentation is docs/*.md, compiled to documents when the pages import it.
    markdown(),
    tailwindcss(),
    compilerOnlyInTheBrowser(),
    // Every page is prerendered to HTML, which GitHub Pages serves as static files.
    prerender({ entry: "/src/server.efs" }),
  ],
  worker: { format: "es", plugins: () => [effscript()] },
  build: { target: "es2022" },
  optimizeDeps: { exclude: ["@effscript/compiler"] },
  server: { fs: { allow: [".."] } },
})
