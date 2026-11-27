import rollupPlugin from "@effscript/rollup"

/** A worker of an `.efs` module, as the dev server requests it. */
const efsWorker = /\.efs\?(?:[^#]*&)?worker_file(?:[&=#]|$)/

/**
 * Compiles `.efs` modules in Vite, before other transforms see them.
 * @param {import("./index.d.ts").Options} [options]
 */
export default function effscript(options = {}) {
  return {
    ...rollupPlugin(options),
    enforce: "pre",
    configureServer(server) {
      // The dev server transforms only the files it knows as JavaScript, and the ones a
      // module imports; a worker is requested by its URL, so mark it as an import.
      server.middlewares.use((req, _res, next) => {
        if (req.url !== undefined && efsWorker.test(req.url)) {
          req.url += "&import"
        }
        next()
      })
    },
  }
}
