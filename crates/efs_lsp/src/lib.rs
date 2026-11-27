//! The EffScript language server.
//!
//! - Diagnostics: syntax, codegen and lint errors as you type; type errors from tsrs
//!   when a file is opened or saved.
//! - Hover: the TypeScript type at the cursor, computed by tsrs on the generated code.
//! - Formatting, document symbols and go-to-definition within a file.
//! - The `efs.showGeneratedTypeScript` command returns the TypeScript for a file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use efs_syntax::ast::*;
use efs_syntax::{LineIndex, Span};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::*;
use serde_json::Value;

pub const SHOW_TS_COMMAND: &str = "efs.showGeneratedTypeScript";

pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let s = uri.as_str().strip_prefix("file://")?;
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    let mut path = String::from_utf8(out).ok()?;
    // file:///C:/x on Windows.
    if path.len() > 2 && path.as_bytes()[0] == b'/' && path.as_bytes()[2] == b':' {
        path.remove(0);
    }
    Some(PathBuf::from(path))
}

pub fn path_to_uri(path: &Path) -> Option<Uri> {
    let mut s = String::from("file://");
    let p = path.to_string_lossy().replace('\\', "/");
    if !p.starts_with('/') {
        s.push('/');
    }
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~:".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s.parse().ok()
}

fn range(index: &LineIndex, span: Span) -> Range {
    let (l1, c1) = index.position(span.start);
    let (l2, c2) = index.position(span.end);
    Range::new(Position::new(l1, c1), Position::new(l2, c2))
}

struct Doc {
    text: String,
    version: i32,
}

pub struct Server {
    conn: Connection,
    docs: HashMap<PathBuf, Doc>,
    sessions: HashMap<PathBuf, efs_check::Session>,
    /// Type diagnostics from the last check, by file.
    type_diags: HashMap<PathBuf, Vec<Diagnostic>>,
}

fn to_lsp_severity(s: efs_syntax::Severity) -> DiagnosticSeverity {
    match s {
        efs_syntax::Severity::Error => DiagnosticSeverity::ERROR,
        efs_syntax::Severity::Warning => DiagnosticSeverity::WARNING,
        efs_syntax::Severity::Info => DiagnosticSeverity::HINT,
    }
}

impl Server {
    pub fn capabilities() -> ServerCapabilities {
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                ..Default::default()
            })),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            definition_provider: Some(OneOf::Left(true)),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: vec![SHOW_TS_COMMAND.into()],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Runs the server over stdio until the client exits.
    pub fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (conn, io) = Connection::stdio();
        let caps = serde_json::to_value(Self::capabilities())?;
        conn.initialize(caps)?;
        {
            let mut server =
                Server { conn, docs: HashMap::new(), sessions: HashMap::new(), type_diags: HashMap::new() };
            server.main_loop()?;
            // Dropping the server closes the connection so that the writer thread can exit.
        }
        io.join()?;
        Ok(())
    }

    fn main_loop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        while let Ok(msg) = self.conn.receiver.recv() {
            match msg {
                Message::Request(req) => {
                    if self.conn.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    let id = req.id.clone();
                    let resp = match self.request(req) {
                        Ok(v) => Response::new_ok(id, v),
                        Err(e) => Response::new_err(id, lsp_server::ErrorCode::RequestFailed as i32, e),
                    };
                    self.conn.sender.send(Message::Response(resp))?;
                }
                Message::Notification(n) => self.notification(n),
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn session(&mut self, path: &Path) -> &mut efs_check::Session {
        let ws = efs_check::Workspace::discover(path.parent().unwrap_or(path));
        self.sessions.entry(ws.root.clone()).or_insert_with(|| efs_check::Session::new(ws))
    }

    fn notification(&mut self, n: Notification) {
        match n.method.as_str() {
            notification::DidOpenTextDocument::METHOD => {
                let Ok(p) = serde_json::from_value::<DidOpenTextDocumentParams>(n.params) else { return };
                let Some(path) = uri_to_path(&p.text_document.uri) else { return };
                self.docs
                    .insert(path.clone(), Doc { text: p.text_document.text.clone(), version: p.text_document.version });
                self.session(&path).set_override(&path, Some(p.text_document.text));
                self.type_check(&path);
            }
            notification::DidChangeTextDocument::METHOD => {
                let Ok(p) = serde_json::from_value::<DidChangeTextDocumentParams>(n.params) else { return };
                let Some(path) = uri_to_path(&p.text_document.uri) else { return };
                if let Some(change) = p.content_changes.into_iter().last() {
                    self.docs.insert(path.clone(), Doc { text: change.text.clone(), version: p.text_document.version });
                    self.session(&path).set_override(&path, Some(change.text));
                    self.publish(&path);
                }
            }
            notification::DidSaveTextDocument::METHOD => {
                let Ok(p) = serde_json::from_value::<DidSaveTextDocumentParams>(n.params) else { return };
                if let Some(path) = uri_to_path(&p.text_document.uri) {
                    self.type_check(&path);
                }
            }
            notification::DidCloseTextDocument::METHOD => {
                let Ok(p) = serde_json::from_value::<DidCloseTextDocumentParams>(n.params) else { return };
                if let Some(path) = uri_to_path(&p.text_document.uri) {
                    self.docs.remove(&path);
                    self.session(&path).set_override(&path, None);
                }
            }
            _ => {}
        }
    }

    /// Runs tsrs over the file's project and publishes diagnostics for every EffScript file in it.
    fn type_check(&mut self, path: &Path) {
        let session = self.session(path);
        let diags = session.type_diagnostics(true);
        let mut by_file: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
        for f in session.compile().keys() {
            by_file.entry(f.clone()).or_default();
        }
        for d in diags {
            let (Some(file), Some(span), Some(text)) = (d.file.clone(), d.span, d.text.clone()) else { continue };
            let index = LineIndex::new(&text);
            by_file.entry(file).or_default().push(Diagnostic {
                range: range(&index, span),
                severity: Some(match d.severity {
                    efs_check::Severity::Error => DiagnosticSeverity::ERROR,
                    efs_check::Severity::Warning => DiagnosticSeverity::WARNING,
                    efs_check::Severity::Suggestion => DiagnosticSeverity::HINT,
                }),
                code: Some(NumberOrString::String(d.code.clone())),
                source: Some("efs (types)".into()),
                message: d.message.clone(),
                ..Default::default()
            });
        }
        let files: Vec<PathBuf> = by_file.keys().cloned().collect();
        self.type_diags.extend(by_file);
        for f in files {
            self.publish(&f);
        }
    }

    fn text_of(&self, path: &Path) -> Option<String> {
        self.docs.get(path).map(|d| d.text.clone()).or_else(|| std::fs::read_to_string(path).ok())
    }

    /// Publishes syntax, codegen and lint diagnostics, plus the last type diagnostics.
    fn publish(&mut self, path: &Path) {
        let Some(text) = self.text_of(path) else { return };
        let index = LineIndex::new(&text);
        let parsed = efs_syntax::parse(&text);
        let out = efs_codegen::compile_module(&text, &parsed.module, &Default::default());
        let mut diags: Vec<Diagnostic> = Vec::new();
        let mut local_spans = Vec::new();
        for d in parsed.diagnostics.iter().chain(out.diagnostics.iter()) {
            local_spans.push(d.span);
            diags.push(Diagnostic {
                range: range(&index, d.span),
                severity: Some(to_lsp_severity(d.severity)),
                code: Some(NumberOrString::String(d.code.clone())),
                source: Some("efs".into()),
                message: d.message.clone(),
                ..Default::default()
            });
        }
        if parsed.diagnostics.is_empty() {
            for d in efs_lint::lint(&text, &parsed.module, &Default::default()) {
                diags.push(Diagnostic {
                    range: range(&index, d.span),
                    severity: Some(to_lsp_severity(d.severity)),
                    code: Some(NumberOrString::String(d.code.clone())),
                    source: Some("efs (lint)".into()),
                    message: d.message.clone(),
                    ..Default::default()
                });
            }
        }
        // Type diagnostics are shown until the next check; skip those on lines with an EffScript error.
        if let Some(t) = self.type_diags.get(path) {
            for d in t {
                let overlaps = local_spans.iter().any(|s| index.position(s.start).0 == d.range.start.line);
                if !overlaps {
                    diags.push(d.clone());
                }
            }
        }
        let Some(uri) = path_to_uri(path) else { return };
        let version = self.docs.get(path).map(|d| d.version);
        let params = PublishDiagnosticsParams { uri, diagnostics: diags, version };
        let _ = self
            .conn
            .sender
            .send(Message::Notification(Notification::new(notification::PublishDiagnostics::METHOD.into(), params)));
    }

    fn request(&mut self, req: Request) -> Result<Value, String> {
        match req.method.as_str() {
            request::Formatting::METHOD => {
                let p: DocumentFormattingParams = serde_json::from_value(req.params).map_err(|e| e.to_string())?;
                let path = uri_to_path(&p.text_document.uri).ok_or("bad uri")?;
                let text = self.text_of(&path).ok_or("unknown document")?;
                let Ok(formatted) = efs_fmt::format(&text) else { return Ok(Value::Null) };
                if formatted == text {
                    return to_json_value(Vec::<TextEdit>::new());
                }
                let index = LineIndex::new(&text);
                let end = range(&index, Span::new(0, text.len() as u32)).end;
                to_json_value(vec![TextEdit { range: Range::new(Position::new(0, 0), end), new_text: formatted }])
            }
            request::HoverRequest::METHOD => {
                let p: HoverParams = serde_json::from_value(req.params).map_err(|e| e.to_string())?;
                let path = uri_to_path(&p.text_document_position_params.text_document.uri).ok_or("bad uri")?;
                let text = self.text_of(&path).ok_or("unknown document")?;
                let pos = p.text_document_position_params.position;
                let offset = LineIndex::new(&text).offset(pos.line, pos.character);
                let Some(info) = self.session(&path).hover(&path, offset) else { return Ok(Value::Null) };
                to_json_value(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```ts\n{info}\n```"),
                    }),
                    range: None,
                })
            }
            request::DocumentSymbolRequest::METHOD => {
                let p: DocumentSymbolParams = serde_json::from_value(req.params).map_err(|e| e.to_string())?;
                let path = uri_to_path(&p.text_document.uri).ok_or("bad uri")?;
                let text = self.text_of(&path).ok_or("unknown document")?;
                to_json_value(DocumentSymbolResponse::Nested(symbols(&text)))
            }
            request::GotoDefinition::METHOD => {
                let p: GotoDefinitionParams = serde_json::from_value(req.params).map_err(|e| e.to_string())?;
                let uri = p.text_document_position_params.text_document.uri.clone();
                let path = uri_to_path(&uri).ok_or("bad uri")?;
                let text = self.text_of(&path).ok_or("unknown document")?;
                let index = LineIndex::new(&text);
                let pos = p.text_document_position_params.position;
                let offset = index.offset(pos.line, pos.character);
                match definition(&text, offset) {
                    Some(span) => {
                        to_json_value(GotoDefinitionResponse::Scalar(Location { uri, range: range(&index, span) }))
                    }
                    None => Ok(Value::Null),
                }
            }
            request::ExecuteCommand::METHOD => {
                let p: ExecuteCommandParams = serde_json::from_value(req.params).map_err(|e| e.to_string())?;
                if p.command != SHOW_TS_COMMAND {
                    return Err(format!("unknown command {}", p.command));
                }
                let uri: Uri = p
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .ok_or("expected a document URI")?;
                let path = uri_to_path(&uri).ok_or("bad uri")?;
                let text = self.text_of(&path).ok_or("unknown document")?;
                Ok(Value::String(efs_codegen::compile(&text, &Default::default()).code))
            }
            _ => Err(format!("unsupported request {}", req.method)),
        }
    }
}

fn to_json_value<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

#[allow(deprecated)]
fn symbol(
    name: &str,
    detail: &str,
    kind: SymbolKind,
    full: Range,
    sel: Range,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name: name.to_string(),
        detail: Some(detail.to_string()),
        kind,
        tags: None,
        deprecated: None,
        range: full,
        selection_range: sel,
        children: if children.is_empty() { None } else { Some(children) },
    }
}

/// The outline of a file.
pub fn symbols(text: &str) -> Vec<DocumentSymbol> {
    let parsed = efs_syntax::parse(text);
    let index = LineIndex::new(text);
    let r = |s: Span| range(&index, s);
    let mut out = Vec::new();
    fn states(body: &StateBody, r: &dyn Fn(Span) -> Range) -> Vec<DocumentSymbol> {
        body.states
            .iter()
            .map(|s| {
                symbol(&s.name.name, "state", SymbolKind::ENUM_MEMBER, r(s.span), r(s.name.span), states(&s.body, r))
            })
            .collect()
    }
    for item in &parsed.module.items {
        let (name, detail, kind, children) = match &item.kind {
            ItemKind::Fn(f) => {
                (&f.name, if f.func.effect { "effect function" } else { "function" }, SymbolKind::FUNCTION, vec![])
            }
            ItemKind::Let(LetDecl {
                kind,
                pattern: efs_syntax::ast::Pattern { kind: PatternKind::Ident(id), .. },
                ..
            }) => {
                let constant = *kind == LetKind::Const;
                (
                    id,
                    if constant { "const" } else { "let" },
                    if constant { SymbolKind::CONSTANT } else { SymbolKind::VARIABLE },
                    vec![],
                )
            }
            ItemKind::Type(t) => (&t.name, "type", SymbolKind::TYPE_PARAMETER, vec![]),
            ItemKind::Service(s) => (&s.name, "service", SymbolKind::INTERFACE, vec![]),
            ItemKind::Layer(l) => (&l.name, "layer", SymbolKind::OBJECT, vec![]),
            ItemKind::Error(e) => (&e.name, "error", SymbolKind::CLASS, vec![]),
            ItemKind::Schema(s) => (&s.name, "schema", SymbolKind::STRUCT, vec![]),
            ItemKind::Enum(e) => (
                &e.name,
                "enum",
                SymbolKind::ENUM,
                e.variants
                    .iter()
                    .map(|v| {
                        symbol(&v.name.name, "variant", SymbolKind::ENUM_MEMBER, r(v.span), r(v.name.span), vec![])
                    })
                    .collect(),
            ),
            ItemKind::Machine(m) => (&m.name, "machine", SymbolKind::CLASS, states(&m.body, &r)),
            ItemKind::Component(c) => (
                &c.name,
                "component",
                SymbolKind::CLASS,
                c.members
                    .iter()
                    .filter_map(|m| {
                        let (name, detail, kind) = match &m.kind {
                            MemberKind::Prop(f) => (&f.name, "prop", SymbolKind::PROPERTY),
                            MemberKind::Let(f) => (&f.name, "model field", SymbolKind::FIELD),
                            MemberKind::Event(v) => (&v.name, "event", SymbolKind::EVENT),
                            MemberKind::Update(u) => (&u.message, "update", SymbolKind::METHOD),
                            _ => return None,
                        };
                        Some(symbol(&name.name, detail, kind, r(m.span), r(name.span), vec![]))
                    })
                    .collect(),
            ),
            _ => continue,
        };
        out.push(symbol(&name.name, detail, kind, r(item.span), r(name.span), children));
    }
    out
}

/// Finds the top-level declaration (or enclosing local binding) of the identifier at `offset`.
pub fn definition(text: &str, offset: u32) -> Option<Span> {
    let parsed = efs_syntax::parse(text);
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let o = offset as usize;
    let start = text[..o].char_indices().rev().take_while(|&(_, c)| is_word(c)).last().map_or(o, |(i, _)| i);
    let end = text[o..].char_indices().find(|&(_, c)| !is_word(c)).map_or(text.len(), |(i, _)| o + i);
    let word = &text[start..end];
    if word.is_empty() {
        return None;
    }
    // The nearest local binding before the cursor wins.
    let mut locals = Vec::new();
    for item in &parsed.module.items {
        collect_item_bindings(item, &mut locals);
    }
    if let Some(span) =
        locals.iter().filter(|(n, s)| n == word && s.start <= offset).map(|(_, s)| *s).max_by_key(|s| s.start)
        && span.start != start as u32
    {
        return Some(span);
    }
    let mut found = None;
    for item in &parsed.module.items {
        let name = match &item.kind {
            ItemKind::Fn(f) => Some(&f.name),
            ItemKind::Let(LetDecl {
                pattern: efs_syntax::ast::Pattern { kind: PatternKind::Ident(id), .. }, ..
            }) => Some(id),
            ItemKind::Type(t) => Some(&t.name),
            ItemKind::Service(s) => Some(&s.name),
            ItemKind::Layer(l) => Some(&l.name),
            ItemKind::Error(e) => Some(&e.name),
            ItemKind::Schema(s) => Some(&s.name),
            ItemKind::Enum(e) => Some(&e.name),
            ItemKind::Machine(m) => Some(&m.name),
            ItemKind::Component(c) => Some(&c.name),
            ItemKind::Import(imp) => {
                let hit = imp
                    .named
                    .iter()
                    .map(|s| s.alias.as_ref().unwrap_or(&s.name))
                    .chain(imp.default.iter())
                    .chain(imp.namespace.iter())
                    .find(|i| i.name == word);
                if let Some(h) = hit {
                    found = Some(h.span);
                }
                None
            }
            _ => None,
        };
        if let Some(n) = name
            && n.name == word
        {
            return Some(n.span);
        }
    }
    found
}

fn pattern_bindings(p: &efs_syntax::ast::Pattern, out: &mut Vec<(String, Span)>) {
    match &p.kind {
        PatternKind::Ident(id) => out.push((id.name.clone(), id.span)),
        PatternKind::Object { props, rest } => {
            for prop in props {
                match &prop.value {
                    Some(v) => pattern_bindings(v, out),
                    None => out.push((prop.key.name.clone(), prop.key.span)),
                }
            }
            if let Some(r) = rest {
                out.push((r.name.clone(), r.span));
            }
        }
        PatternKind::Array { elems, rest } => {
            for e in elems.iter().flatten() {
                pattern_bindings(e, out);
            }
            if let Some(r) = rest {
                pattern_bindings(r, out);
            }
        }
        PatternKind::Default { pattern, .. } => pattern_bindings(pattern, out),
    }
}

fn func_bindings(f: &Func, out: &mut Vec<(String, Span)>) {
    for p in &f.params {
        pattern_bindings(&p.pattern, out);
    }
    match &f.body {
        FuncBody::Block(b) => block_bindings(b, out),
        FuncBody::Expr(e) => expr_bindings(e, out),
    }
}

fn expr_bindings(e: &Expr, out: &mut Vec<(String, Span)>) {
    efs_syntax::visit::walk_expr(e, &mut |x| match &x.kind {
        ExprKind::Func(f) => {
            for p in &f.params {
                pattern_bindings(&p.pattern, out);
            }
            if let FuncBody::Block(b) = &f.body {
                block_bindings(b, out);
            }
        }
        ExprKind::EffectBlock(b) => block_bindings(b, out),
        _ => {}
    });
}

fn block_bindings(b: &Block, out: &mut Vec<(String, Span)>) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Let(l) => {
                pattern_bindings(&l.pattern, out);
                if let Some(i) = &l.init {
                    expr_bindings(i, out);
                }
            }
            StmtKind::Fn(f) => {
                out.push((f.name.name.clone(), f.name.span));
                func_bindings(&f.func, out);
            }
            StmtKind::For { pattern, iter, body, .. } => {
                pattern_bindings(pattern, out);
                expr_bindings(iter, out);
                block_bindings(body, out);
            }
            StmtKind::Block(b) | StmtKind::While { body: b, .. } => block_bindings(b, out),
            StmtKind::If(i) => {
                block_bindings(&i.then, out);
                if let Some(e) = &i.else_ {
                    block_bindings(&Block { stmts: vec![(**e).clone()], span: e.span }, out);
                }
            }
            StmtKind::Expr(e)
            | StmtKind::Return(Some(e))
            | StmtKind::Fail(e)
            | StmtKind::Defer(e)
            | StmtKind::Emit(e) => expr_bindings(e, out),
            _ => {}
        }
    }
}

fn collect_item_bindings(item: &Item, out: &mut Vec<(String, Span)>) {
    match &item.kind {
        ItemKind::Fn(f) => func_bindings(&f.func, out),
        ItemKind::Let(l) => {
            if let Some(i) = &l.init {
                expr_bindings(i, out);
            }
        }
        ItemKind::Layer(l) => match &l.body {
            LayerBody::Effect(b) => block_bindings(b, out),
            LayerBody::Value(v) => expr_bindings(v, out),
        },
        ItemKind::Stmt(s) => block_bindings(&Block { stmts: vec![s.clone()], span: s.span }, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn uri_round_trip() {
        let p = std::path::Path::new("/tmp/a b/c.efs");
        let u = super::path_to_uri(p).unwrap();
        assert_eq!(u.as_str(), "file:///tmp/a%20b/c.efs");
        assert_eq!(super::uri_to_path(&u).unwrap(), p);
    }

    #[test]
    fn symbols_and_definitions() {
        let src = "service Repo { all: number }\nconst x = Repo\n";
        let syms = super::symbols(src);
        assert_eq!(syms.len(), 2);
        let def = super::definition(src, src.rfind("Repo").unwrap() as u32 + 1).unwrap();
        assert_eq!(def.text(src), "Repo");
        assert_eq!(def.start, 8);
        let local = "service Repo { all: number }\neffect function f(id: string) {\n  const repo = use Repo\n  return repo.find(id)\n}\n";
        let def = super::definition(local, local.rfind("Repo").unwrap() as u32 + 1).unwrap();
        assert_eq!(def.start, 8);
        let def = super::definition(local, local.rfind("repo.find").unwrap() as u32 + 1).unwrap();
        assert_eq!(def.start as usize, local.find("repo").unwrap());
        let def = super::definition(local, local.rfind("(id)").unwrap() as u32 + 1).unwrap();
        assert_eq!(def.start as usize, local.find("id").unwrap());
    }
}
