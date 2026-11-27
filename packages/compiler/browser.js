// The EffScript compiler, running as WebAssembly in the browser.
import { compiler, EffScriptError, formatDiagnostic } from "./core.js"

export { EffScriptError, formatDiagnostic }

/**
 * Loads the compiler. By default `efs.wasm` is fetched from next to this module, which
 * bundlers such as Vite copy into the build; pass a URL, a `Response` or the bytes to load
 * it from elsewhere.
 * @param {string | URL | Response | BufferSource} [input]
 */
export async function init(input = new URL("./efs.wasm", import.meta.url)) {
  let instance
  if (input instanceof ArrayBuffer || ArrayBuffer.isView(input)) {
    instance = (await WebAssembly.instantiate(input, {})).instance
  } else {
    const response = input instanceof Response ? input : fetch(input)
    try {
      instance = (await WebAssembly.instantiateStreaming(response, {})).instance
    } catch {
      // Servers that don't send `application/wasm` can't stream.
      const bytes = await (await (input instanceof Response ? input : fetch(input))).arrayBuffer()
      instance = (await WebAssembly.instantiate(bytes, {})).instance
    }
  }
  const exports = instance.exports
  return compiler(() => exports)
}
