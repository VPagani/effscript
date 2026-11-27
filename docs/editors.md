# Editors

## VS Code

`editors/vscode` is a VS Code extension with syntax highlighting and the
EffScript language server. Its "EffScript: Show Generated TypeScript" command
opens the code a file compiles to, beside it. Set `efs.path` if the `efs`
command is not on your `PATH`.

## Other editors

Any editor with LSP support can run `efs lsp`. The language server provides:

- syntax, codegen and lint diagnostics as you type, and type errors from tsrs
  when a file is opened or saved;
- hovers with the types tsrs infers;
- formatting, document symbols, and go to definition for local and top-level
  names;
- the `efs.showGeneratedTypeScript` command, which returns the generated code
  for a file.

## TypeScript's own language server

For completions, rename and the rest of TypeScript's editor features in `.efs`
files, enable the [content mapper](typescript.md#type-checking): the
TypeScript language server then reads `.efs` files directly. Through
`effect-tsgo`, the Effect Language Service's diagnostics and quick fixes apply
too.
