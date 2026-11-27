import { copyFileSync, mkdirSync } from "node:fs"
import tailwindcss from "@tailwindcss/vite"
import effscript from "@effscript/vite"
import { defineConfig } from "vite"

/**
 * GitHub Pages serves static files, so each route gets a copy of the app's page, and
 * `404.html` serves the app for any other path.
 */
const routes = (paths) => ({
  name: "static-routes",
  apply: "build",
  closeBundle() {
    const index = new URL("./dist/index.html", import.meta.url)
    for (const path of paths) {
      mkdirSync(new URL(`./dist/${path}/`, import.meta.url), { recursive: true })
      copyFileSync(index, new URL(`./dist/${path}/index.html`, import.meta.url))
    }
    copyFileSync(index, new URL("./dist/404.html", import.meta.url))
  },
})

export default defineConfig({
  plugins: [effscript(), tailwindcss(), routes(["playground"])],
  worker: { format: "es", plugins: () => [effscript()] },
  build: { target: "es2022" },
  optimizeDeps: { exclude: ["@effscript/compiler"] },
})
