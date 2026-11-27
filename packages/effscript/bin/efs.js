#!/usr/bin/env node
// Runs the native `efs` binary for this platform with the same arguments and stdio.
"use strict"

const { spawnSync } = require("node:child_process")
const path = require("node:path")
const fs = require("node:fs")

function findBinary() {
  if (process.env.EFS_BINARY) return process.env.EFS_BINARY
  const exe = process.platform === "win32" ? "efs.exe" : "efs"
  const local = path.join(__dirname, "..", "native", exe)
  if (fs.existsSync(local)) return local
  const pkg = `@effscript/cli-${process.platform}-${process.arch}`
  try {
    return path.join(path.dirname(require.resolve(`${pkg}/package.json`)), exe)
  } catch {
    console.error(
      `efs: no binary for ${process.platform}-${process.arch}. Install ${pkg}, ` +
        "build one with `cargo install --path crates/efs_cli`, or set EFS_BINARY."
    )
    process.exit(1)
  }
}

const result = spawnSync(findBinary(), process.argv.slice(2), { stdio: "inherit" })
if (result.error) {
  console.error(`efs: ${result.error.message}`)
  process.exit(1)
}
process.exit(result.status ?? 1)
