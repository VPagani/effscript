// Builds the fixture with every bundler plugin, runs each output with Node, and checks that
// its source map points back to the .efs sources.
import { execFileSync, spawnSync } from "node:child_process"
import { existsSync, readdirSync, readFileSync, rmSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { build as esbuild } from "esbuild"
import { rolldown } from "rolldown"
import { rollup } from "rollup"
import { build as vite } from "vite"
import webpack from "webpack"
import effscriptEsbuild from "@effscript/esbuild"
import effscriptRolldown from "@effscript/rolldown"
import effscriptRollup from "@effscript/rollup"
import effscriptVite from "@effscript/vite"

const here = fileURLToPath(new URL(".", import.meta.url))
const entry = `${here}src/main.js`
const external = (id) => id === "effect" || id.startsWith("effect/")
const expected = "Hello, bundlers and plugins! (2)"

const builds = {
  async esbuild(dir) {
    await esbuild({
      entryPoints: [entry], outdir: dir, bundle: true, format: "esm", platform: "node", sourcemap: true,
      external: ["effect"], plugins: [effscriptEsbuild()], logLevel: "silent",
    })
  },
  async rollup(dir) {
    const bundle = await rollup({ input: entry, external, plugins: [effscriptRollup()] })
    await bundle.write({ dir, format: "es", sourcemap: true })
  },
  async rolldown(dir) {
    const bundle = await rolldown({ input: entry, external, plugins: [effscriptRolldown()] })
    await bundle.write({ dir, format: "es", sourcemap: true })
  },
  async vite(dir) {
    await vite({
      configFile: false, logLevel: "silent", root: here, plugins: [effscriptVite()],
      build: { ssr: entry, outDir: dir, sourcemap: true, emptyOutDir: true, rollupOptions: { output: { entryFileNames: "main.js" } } },
    })
  },
  async webpack(dir) {
    const stats = await new Promise((resolve, reject) =>
      webpack({
        mode: "none", entry, target: "node", devtool: "source-map",
        output: { path: dir, filename: "main.js", module: true, chunkFormat: "module", library: { type: "module" } },
        experiments: { outputModule: true, topLevelAwait: true },
        externalsType: "module", externals: [({ request }, done) => (external(request) ? done(null, request) : done())],
        module: { rules: [{ test: /\.efs$/, use: fileURLToPath(import.meta.resolve("@effscript/webpack")) }] },
      }, (err, stats) => (err ? reject(err) : resolve(stats))),
    )
    if (stats.hasErrors()) throw new Error(stats.toString("errors-only"))
  },
  async bun(dir) {
    if (spawnSync("bun", ["--version"]).status !== 0) return "skipped (bun is not installed)"
    execFileSync("bun", ["run", "bun-build.mjs"], { cwd: here, stdio: "inherit" })
    if (`${here}dist/bun` !== dir) throw new Error("unexpected output directory")
  },
}

let failed = false
for (const [name, build] of Object.entries(builds)) {
  const dir = `${here}dist/${name}`
  rmSync(dir, { recursive: true, force: true })
  try {
    const note = await build(dir)
    if (note) {
      console.log(`- ${name}: ${note}`)
      continue
    }
    const main = readdirSync(dir).find((f) => f.endsWith(".js") || f.endsWith(".mjs"))
    const out = execFileSync("node", [`${dir}/${main}`], { encoding: "utf8" }).trim()
    if (out !== expected) throw new Error(`printed ${JSON.stringify(out)}`)
    const map = `${dir}/${main}.map`
    if (!existsSync(map)) throw new Error("no source map")
    const sources = JSON.parse(readFileSync(map, "utf8")).sources
    if (!sources.some((s) => s.endsWith("app.efs")) || !sources.some((s) => s.endsWith("greeting.efs"))) {
      throw new Error(`the source map does not point to the .efs files: ${sources.join(", ")}`)
    }
    console.log(`✓ ${name}`)
  } catch (e) {
    failed = true
    console.log(`✗ ${name}: ${e.message}`)
  }
}
process.exit(failed ? 1 : 0)
