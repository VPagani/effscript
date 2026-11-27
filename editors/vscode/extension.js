// @ts-check
const vscode = require("vscode")
const { LanguageClient } = require("vscode-languageclient/node")

/** @type {LanguageClient | undefined} */
let client

function start() {
  const command = vscode.workspace.getConfiguration("efs").get("path") || "efs"
  client = new LanguageClient(
    "efs",
    "EffScript",
    { command, args: ["lsp"] },
    { documentSelector: [{ scheme: "file", language: "efs" }] }
  )
  return client.start()
}

/** @param {vscode.ExtensionContext} context */
async function activate(context) {
  const scheme = "efs-generated"
  const generated = new Map()
  const changed = new vscode.EventEmitter()
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(scheme, {
      onDidChange: changed.event,
      provideTextDocumentContent: (uri) => generated.get(uri.toString()) ?? ""
    }),
    vscode.commands.registerCommand("efs.showGeneratedTypeScript", async () => {
      const editor = vscode.window.activeTextEditor
      if (!editor || editor.document.languageId !== "efs" || !client) return
      const code = await client.sendRequest("workspace/executeCommand", {
        command: "efs.showGeneratedTypeScript",
        arguments: [editor.document.uri.toString()]
      })
      const target = vscode.Uri.parse(`${scheme}:${editor.document.uri.path.replace(/\.efs$/, ".ts")}`)
      generated.set(target.toString(), code)
      changed.fire(target)
      const doc = await vscode.workspace.openTextDocument(target)
      await vscode.languages.setTextDocumentLanguage(doc, "typescript")
      await vscode.window.showTextDocument(doc, { viewColumn: vscode.ViewColumn.Beside, preview: true })
    }),
    vscode.commands.registerCommand("efs.restartServer", async () => {
      await client?.stop()
      await start()
    })
  )
  await start()
}

function deactivate() {
  return client?.stop()
}

module.exports = { activate, deactivate }
