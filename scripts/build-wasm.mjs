// Builds the WebAssembly compiler and copies it into @effscript/compiler.
import { execFileSync } from "node:child_process"
import { copyFileSync } from "node:fs"
import { fileURLToPath } from "node:url"

const root = fileURLToPath(new URL("..", import.meta.url))
execFileSync("cargo", ["build", "-q", "-p", "efs_wasm", "--target", "wasm32-unknown-unknown", "--release"], {
  cwd: root,
  stdio: "inherit",
})
copyFileSync(
  new URL("../target/wasm32-unknown-unknown/release/efs_wasm.wasm", import.meta.url),
  new URL("../packages/compiler/efs.wasm", import.meta.url),
)
