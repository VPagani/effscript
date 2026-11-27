// Run with Bun: builds the fixture with Bun's bundler and the EffScript plugin.
import effscript from "@effscript/bun"

const result = await Bun.build({
  entrypoints: ["src/main.js"],
  outdir: "dist/bun",
  target: "node",
  format: "esm",
  sourcemap: "external",
  external: ["effect"],
  plugins: [effscript()],
})
if (!result.success) {
  for (const log of result.logs) console.error(log)
  process.exit(1)
}
