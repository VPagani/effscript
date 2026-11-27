//! A hand-written recursive-descent parser. Newlines terminate statements, except
//! inside brackets or when the next line clearly continues an expression.

use std::collections::BTreeMap;

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::lexer::{Comment, Lexer, Token, TokenKind};
use crate::span::Span;

pub struct ParseResult {
    pub module: Module,
    pub comments: Vec<Comment>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(src: &str) -> ParseResult {
    let mut p = Parser::new(src);
    let module = p.module();
    ParseResult { module, comments: p.comments.into_values().collect(), diagnostics: p.diags }
}

/// Parses a single expression (used by tests and tooling).
pub fn parse_expr(src: &str) -> (Expr, Vec<Diagnostic>) {
    let mut p = Parser::new(src);
    let e = p.expr();
    if p.peek().kind != TokenKind::Eof {
        let t = p.peek();
        p.error(t.span, "unexpected token after expression");
    }
    (e, p.diags)
}

#[derive(Clone)]
struct Checkpoint {
    lexer: Lexer,
    buf: Vec<(Token, Lexer)>,
    last_lexer: Lexer,
    prev_end: u32,
    diags: usize,
}

pub struct Parser<'a> {
    src: &'a str,
    lexer: Lexer,
    /// Lookahead tokens, each with the lexer state after it.
    buf: Vec<(Token, Lexer)>,
    last_lexer: Lexer,
    prev_end: u32,
    comments: BTreeMap<u32, Comment>,
    diags: Vec<Diagnostic>,
    /// Inside an effect body, where `run` is a keyword.
    in_effect: bool,
    /// Inside a component update block, where `run` issues a command and `emit` sends an event.
    in_update: bool,
}

const BIN_PREC: &[(&str, u8)] = &[
    ("??", 1),
    ("||", 2),
    ("&&", 3),
    ("|", 4),
    ("^", 5),
    ("&", 6),
    ("==", 7),
    ("!=", 7),
    ("===", 7),
    ("!==", 7),
    ("<", 8),
    (">", 8),
    ("<=", 8),
    (">=", 8),
    ("instanceof", 8),
    ("in", 8),
    ("as", 8),
    ("satisfies", 8),
    ("<<", 9),
    (">>", 9),
    (">>>", 9),
    ("+", 10),
    ("-", 10),
    ("*", 11),
    ("/", 11),
    ("%", 11),
    ("**", 12),
];

const ASSIGN_OPS: &[&str] = &["=", "+=", "-=", "*=", "/=", "%=", "**=", "<<=", "&=", "|=", "^=", "&&=", "||=", "??="];

impl<'a> Parser<'a> {
    pub fn new(src: &'a str) -> Parser<'a> {
        Parser {
            src,
            lexer: Lexer::new(),
            buf: Vec::new(),
            last_lexer: Lexer::new(),
            prev_end: 0,
            comments: BTreeMap::new(),
            diags: Vec::new(),
            in_effect: false,
            in_update: false,
        }
    }

    // ---------------------------------------------------------------------------------------
    // Token plumbing
    // ---------------------------------------------------------------------------------------

    fn fill(&mut self, n: usize) {
        while self.buf.len() <= n {
            let t = self.lexer.next(self.src, &mut self.comments);
            self.buf.push((t, self.lexer.clone()));
        }
    }

    fn peek(&mut self) -> Token {
        self.peek_n(0)
    }

    fn peek_n(&mut self, n: usize) -> Token {
        self.fill(n);
        self.buf[n].0
    }

    fn bump(&mut self) -> Token {
        self.fill(0);
        let (t, lx) = self.buf.remove(0);
        if t.kind == TokenKind::Error {
            self.error(t.span, t.error.unwrap_or("invalid token"));
        }
        if t.kind != TokenKind::Eof {
            self.prev_end = t.span.end;
        }
        self.last_lexer = lx;
        t
    }

    /// Discards lookahead and resumes lexing right after the last consumed token.
    fn reset_after_last(&mut self) {
        self.buf.clear();
        self.lexer = self.last_lexer.clone();
        self.lexer.pos = self.prev_end as usize;
    }

    fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            lexer: self.lexer.clone(),
            buf: self.buf.clone(),
            last_lexer: self.last_lexer.clone(),
            prev_end: self.prev_end,
            diags: self.diags.len(),
        }
    }

    fn restore(&mut self, c: Checkpoint) {
        self.lexer = c.lexer;
        self.buf = c.buf;
        self.last_lexer = c.last_lexer;
        self.prev_end = c.prev_end;
        self.diags.truncate(c.diags);
    }

    fn text(&self, t: Token) -> &'a str {
        t.span.text(self.src)
    }

    fn is(&mut self, s: &str) -> bool {
        let t = self.peek();
        matches!(t.kind, TokenKind::Punct | TokenKind::Ident) && self.text(t) == s
    }

    fn is_n(&mut self, n: usize, s: &str) -> bool {
        let t = self.peek_n(n);
        matches!(t.kind, TokenKind::Punct | TokenKind::Ident) && self.text(t) == s
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, s: &str) -> Span {
        if self.is(s) {
            self.bump().span
        } else {
            let t = self.peek();
            let found =
                if t.kind == TokenKind::Eof { "end of file".to_string() } else { format!("`{}`", self.text(t)) };
            self.error(t.span, format!("expected `{s}`, found {found}"));
            Span::new(t.span.start, t.span.start)
        }
    }

    fn error(&mut self, span: Span, msg: impl Into<String>) {
        let msg = msg.into();
        // Avoid cascades of identical errors at one position.
        if self.diags.last().is_some_and(|d| d.span.start == span.start) {
            return;
        }
        self.diags.push(Diagnostic::error(span, msg));
    }

    fn is_ident(&mut self) -> bool {
        self.peek().kind == TokenKind::Ident
    }

    fn ident(&mut self) -> Ident {
        let t = self.peek();
        if t.kind == TokenKind::Ident {
            self.bump();
            Ident { name: self.text(t).to_string(), span: t.span }
        } else {
            self.error(t.span, "expected an identifier");
            Ident { name: "__error".into(), span: Span::new(t.span.start, t.span.start) }
        }
    }

    fn start(&mut self) -> u32 {
        self.peek().span.start
    }

    fn span_from(&self, start: u32) -> Span {
        Span::new(start, self.prev_end.max(start))
    }

    /// Optional separators between list entries.
    fn eat_seps(&mut self) {
        while self.eat(",") || self.eat(";") {}
    }

    fn at_eof(&mut self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    /// Whether the next token is a word followed by an identifier on the same line.
    fn is_kw_then_ident(&mut self, kw: &str) -> bool {
        self.is(kw) && {
            let t = self.peek_n(1);
            t.kind == TokenKind::Ident && !t.nl_before
        }
    }

    fn end_stmt(&mut self) {
        if self.eat(";") {
            return;
        }
        let t = self.peek();
        if t.kind == TokenKind::Eof || t.nl_before || self.is("}") {
            return;
        }
        self.error(t.span, format!("expected a new line or `;` before `{}`", self.text(t)));
        self.recover();
    }

    fn recover(&mut self) {
        loop {
            let t = self.peek();
            if t.kind == TokenKind::Eof || self.is("}") {
                return;
            }
            self.bump();
            if self.peek().nl_before {
                return;
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Module and items
    // ---------------------------------------------------------------------------------------

    fn module(&mut self) -> Module {
        let mut items = Vec::new();
        while !self.at_eof() {
            let before = self.peek().span.start;
            if self.eat(";") {
                continue;
            }
            if self.is("}") {
                let t = self.bump();
                self.error(t.span, "unexpected `}`");
                continue;
            }
            let item = self.item();
            items.push(item);
            self.end_stmt();
            if self.peek().span.start == before && !self.at_eof() {
                self.bump();
            }
        }
        Module { items, span: Span::new(0, self.src.len() as u32) }
    }

    fn item(&mut self) -> Item {
        let start = self.start();
        let mut export = false;
        if self.is("export") {
            self.bump();
            export = true;
            if self.is("{") || self.is("*") {
                let list = self.export_list();
                return Item { export, kind: ItemKind::ExportList(list), span: self.span_from(start) };
            }
            if self.is("default") {
                let t = self.bump();
                self.error(t.span, "`export default` is not supported; export a named binding instead");
            }
        }
        let kind = self.item_kind();
        if export && matches!(&kind, ItemKind::Stmt(_) | ItemKind::Import(_) | ItemKind::ExportList(_)) {
            let span = self.span_from(start);
            self.error(span, "this cannot be exported");
        }
        Item { export, kind, span: self.span_from(start) }
    }

    fn item_kind(&mut self) -> ItemKind {
        if self.is("import") && !self.is_n(1, "(") && !self.is_n(1, ".") {
            return ItemKind::Import(self.import());
        }
        if self.is("const") || self.is("let") || self.is("var") {
            return ItemKind::Let(self.let_decl());
        }
        if self.is("function") && self.peek_n(1).kind == TokenKind::Ident {
            return ItemKind::Fn(self.fn_decl(false));
        }
        if self.is("effect") && self.is_n(1, "function") && self.peek_n(2).kind == TokenKind::Ident {
            self.bump();
            return ItemKind::Fn(self.fn_decl(true));
        }
        if self.is_kw_then_ident("type") {
            return ItemKind::Type(self.type_alias());
        }
        if self.is_kw_then_ident("service") {
            return ItemKind::Service(self.service());
        }
        if self.is_kw_then_ident("error") {
            return ItemKind::Error(self.error_decl());
        }
        if self.is_kw_then_ident("schema") {
            return ItemKind::Schema(self.schema_decl(false));
        }
        if self.is("tagged") && self.is_n(1, "schema") {
            self.bump();
            return ItemKind::Schema(self.schema_decl(true));
        }
        if self.is_kw_then_ident("enum") {
            return ItemKind::Enum(self.enum_decl());
        }
        if self.is_kw_then_ident("machine") {
            return ItemKind::Machine(self.machine());
        }
        if self.is_kw_then_ident("component") {
            return ItemKind::Component(self.component());
        }
        ItemKind::Stmt(self.stmt())
    }

    fn string_lit(&mut self) -> StrLit {
        let t = self.peek();
        if t.kind == TokenKind::String {
            self.bump();
            StrLit { raw: self.text(t).to_string(), span: t.span }
        } else {
            self.error(t.span, "expected a string literal");
            StrLit { raw: "\"\"".into(), span: Span::new(t.span.start, t.span.start) }
        }
    }

    fn import(&mut self) -> Import {
        self.expect("import");
        let mut imp = Import {
            type_only: false,
            default: None,
            namespace: None,
            named: Vec::new(),
            has_braces: false,
            source: StrLit { raw: String::new(), span: Span::DUMMY },
        };
        if self.peek().kind == TokenKind::String {
            imp.source = self.string_lit();
            return imp;
        }
        if self.is("type") && !self.is_n(1, "from") && !self.is_n(1, ",") {
            self.bump();
            imp.type_only = true;
        }
        if self.is_ident() && !self.is("from") {
            imp.default = Some(self.ident());
            self.eat(",");
        }
        if self.eat("*") {
            self.expect("as");
            imp.namespace = Some(self.ident());
        } else if self.is("{") {
            imp.has_braces = true;
            imp.named = self.spec_list();
        }
        self.expect("from");
        imp.source = self.string_lit();
        imp
    }

    fn spec_list(&mut self) -> Vec<ImportSpec> {
        self.expect("{");
        let mut specs = Vec::new();
        while !self.is("}") && !self.at_eof() {
            let mut type_only = false;
            if self.is("type") && self.peek_n(1).kind == TokenKind::Ident && !self.is_n(1, "as") {
                self.bump();
                type_only = true;
            }
            let name = self.ident();
            let alias = if self.eat("as") { Some(self.ident()) } else { None };
            specs.push(ImportSpec { type_only, name, alias });
            if !self.eat(",") {
                break;
            }
        }
        self.expect("}");
        specs
    }

    fn export_list(&mut self) -> ExportList {
        let mut list = ExportList { star: false, specs: Vec::new(), source: None };
        if self.eat("*") {
            list.star = true;
        } else {
            list.specs = self.spec_list();
        }
        if self.eat("from") {
            list.source = Some(self.string_lit());
        } else if list.star {
            let t = self.peek();
            self.error(t.span, "expected `from`");
        }
        list
    }

    fn let_decl(&mut self) -> LetDecl {
        let kw = self.bump();
        let kind = match kw.span.text(self.src) {
            "const" => LetKind::Const,
            "var" => {
                self.error(kw.span, "`var` is not supported; use `let` for a mutable binding or `const`");
                LetKind::Let
            }
            _ => LetKind::Let,
        };
        let pattern = self.pattern();
        let ty = if self.eat(":") { Some(self.ty()) } else { None };
        let init = if self.eat("=") {
            Some(self.expr())
        } else {
            if kind == LetKind::Const {
                let t = self.peek();
                self.error(t.span, "`const` bindings need a value; use `let` to assign one later");
            }
            None
        };
        LetDecl { kind, pattern, ty, init }
    }

    fn fn_decl(&mut self, effect: bool) -> FnDecl {
        let start = self.start();
        self.expect("function");
        let name = self.ident();
        let func = self.func_rest(effect, start);
        FnDecl { name, func }
    }

    /// Parses `<T>(params): Ret { body }`.
    fn func_rest(&mut self, effect: bool, start: u32) -> Func {
        let type_params = self.type_params();
        let params = self.params();
        let ret = if self.eat(":") { Some(self.ty()) } else { None };
        let saved = std::mem::replace(&mut self.in_effect, effect);
        let saved_update = std::mem::replace(&mut self.in_update, false);
        let body = FuncBody::Block(self.block());
        self.in_effect = saved;
        self.in_update = saved_update;
        Func { effect, arrow: false, type_params, params, ret, body, span: self.span_from(start) }
    }

    /// A block that is an effect body.
    fn effect_block(&mut self) -> Block {
        let saved = std::mem::replace(&mut self.in_effect, true);
        let saved_update = std::mem::replace(&mut self.in_update, false);
        let b = self.block();
        self.in_effect = saved;
        self.in_update = saved_update;
        b
    }

    /// Whether the next token is the `run` keyword. Inside effect bodies `run` is reserved,
    /// like `await` in async functions; elsewhere it is only treated as the keyword when it
    /// cannot be an ordinary identifier, so that a helpful error can be reported.
    fn at_run(&mut self) -> bool {
        if !self.is("run") {
            return false;
        }
        let next = self.peek_n(1);
        if next.nl_before || next.kind == TokenKind::Eof {
            return false;
        }
        let text = self.text(next);
        if next.kind == TokenKind::Punct
            && matches!(text, ")" | "," | "}" | "]" | ";" | ":" | "=" | "=>" | "." | "?." | "|>" | "?")
        {
            return false;
        }
        if self.in_effect || self.in_update {
            return true;
        }
        self.word_starts_expr()
    }

    /// Parses `run expr` (the keyword is the next token).
    fn run_expr(&mut self, stop_at_with: bool) -> Expr {
        let start = self.start();
        let kw = self.bump();
        if !self.in_effect && !self.in_update {
            self.error(
                kw.span,
                "`run` can only be used inside an `effect function`, an `effect` arrow, an `effect { }` block or a component `update`",
            );
        }
        let inner = if stop_at_with { self.expr_no_with() } else { self.expr() };
        Expr { kind: ExprKind::Bind(BindKind::Run, Box::new(inner)), span: self.span_from(start) }
    }

    /// Whether the next token is the `use` keyword. `use` is a keyword only when a name
    /// follows it on the same line (`use UserRepo`); otherwise it is an ordinary identifier,
    /// so `use(x)`, `const use = 1` and `x.use` keep working.
    fn at_use(&mut self) -> bool {
        if !self.is("use") {
            return false;
        }
        let next = self.peek_n(1);
        next.kind == TokenKind::Ident
            && !next.nl_before
            && !matches!(self.text(next), "as" | "satisfies" | "in" | "instanceof" | "of" | "with" | "catch")
    }

    /// Parses `use Service` (the keyword is the next token).
    fn use_expr(&mut self, stop_at_with: bool) -> Expr {
        let start = self.start();
        let kw = self.bump();
        if self.in_update && !self.in_effect {
            self.error(
                kw.span,
                "`use` gets a service inside an effect body; an update cannot get services, use `run` to issue a command",
            );
        } else if !self.in_effect {
            self.error(
                kw.span,
                "`use` can only be used inside an `effect function`, an `effect` arrow or an `effect { }` block",
            );
        }
        let inner = if stop_at_with { self.expr_no_with() } else { self.expr() };
        if inner.name_path().is_none() && !matches!(inner.kind, ExprKind::Error) {
            let text = inner.span.text(self.src);
            let run = if text.len() <= 40 && !text.contains('\n') { format!("run {text}") } else { "run ...".into() };
            self.error(
                inner.span,
                format!("`use` takes the name of a service, such as `use UserRepo`; to run an effect, write `{run}`"),
            );
        }
        Expr { kind: ExprKind::Bind(BindKind::Use, Box::new(inner)), span: self.span_from(start) }
    }

    fn type_alias(&mut self) -> TypeAlias {
        self.expect("type");
        let name = self.ident();
        let type_params = self.type_params();
        self.expect("=");
        let ty = self.ty();
        TypeAlias { name, type_params, ty }
    }

    fn service(&mut self) -> Service {
        self.expect("service");
        let name = self.ident();
        let key = if self.eat("(") {
            let s = self.string_lit();
            self.expect(")");
            Some(s)
        } else {
            None
        };
        let members = self.type_members();
        Service { name, key, members }
    }

    fn error_decl(&mut self) -> ErrorDecl {
        self.expect("error");
        let name = self.ident();
        let fields = if self.is("{") && !self.peek().nl_before { self.type_members() } else { Vec::new() };
        ErrorDecl { name, fields }
    }

    fn schema_decl(&mut self, tagged: bool) -> SchemaDecl {
        self.expect("schema");
        let class = !tagged && self.is("class") && self.peek_n(1).kind == TokenKind::Ident;
        if class {
            self.bump();
        }
        let name = self.ident();
        let kind = if !class && !tagged && self.eat("=") {
            SchemaKind::Alias(self.ty())
        } else {
            let fields = self.schema_fields("{", "}");
            if class {
                SchemaKind::Class(fields)
            } else if tagged {
                SchemaKind::Tagged(fields)
            } else {
                SchemaKind::Struct(fields)
            }
        };
        SchemaDecl { name, kind }
    }

    fn schema_fields(&mut self, open: &str, close: &str) -> Vec<SchemaField> {
        self.expect(open);
        let mut fields = Vec::new();
        self.eat_seps();
        while !self.is(close) && !self.at_eof() {
            let start = self.start();
            let name = self.ident();
            let optional = self.eat("?");
            let value = if self.eat("=") {
                SchemaFieldValue::Expr(self.expr())
            } else {
                self.expect(":");
                SchemaFieldValue::Type(self.ty())
            };
            let default =
                if matches!(value, SchemaFieldValue::Type(_)) && self.eat("=") { Some(self.expr()) } else { None };
            fields.push(SchemaField { name, optional, value, default, span: self.span_from(start) });
            let before = self.peek().span.start;
            self.eat_seps();
            if !self.is(close) && self.peek().span.start == before && !self.peek().nl_before {
                let t = self.peek();
                self.error(t.span, format!("expected `,`, a new line or `{close}`"));
                self.recover();
            }
        }
        self.expect(close);
        fields
    }

    fn variants(&mut self) -> Vec<Variant> {
        self.expect("{");
        let mut out = Vec::new();
        self.eat_seps();
        while !self.is("}") && !self.at_eof() {
            let start = self.start();
            let name = self.ident();
            let fields = if self.is("{") && !self.peek().nl_before { self.schema_fields("{", "}") } else { Vec::new() };
            out.push(Variant { name, fields, span: self.span_from(start) });
            let before = self.peek().span.start;
            self.eat_seps();
            if !self.is("}") && self.peek().span.start == before && !self.peek().nl_before {
                let t = self.peek();
                self.error(t.span, "expected `,`, a new line or `}`");
                self.recover();
            }
        }
        self.expect("}");
        out
    }

    fn enum_decl(&mut self) -> EnumDecl {
        self.expect("enum");
        let name = self.ident();
        let variants = self.variants();
        EnumDecl { name, variants }
    }

    // ---------------------------------------------------------------------------------------
    // Machines
    // ---------------------------------------------------------------------------------------

    fn machine(&mut self) -> Machine {
        self.expect("machine");
        let name = self.ident();
        let parallel = self.eat("parallel");
        let mut m = Machine {
            name,
            parallel,
            input: None,
            root_fields: Vec::new(),
            events: Vec::new(),
            sources: Vec::new(),
            body: StateBody::default(),
        };
        self.expect("{");
        loop {
            self.eat_seps();
            if self.is("}") || self.at_eof() {
                break;
            }
            if self.eat("input") {
                m.input = Some(self.ty());
            } else if self.is("root") && self.is_n(1, "{") {
                self.bump();
                m.root_fields = self.schema_fields("{", "}");
            } else if self.is("events") && self.is_n(1, "{") {
                self.bump();
                m.events = self.variants();
            } else if (self.is("effects") || self.is("streams") || self.is("timers")) && self.is_n(1, "{") {
                let kw = self.bump();
                let kind = match self.text(kw) {
                    "effects" => SourceKind::Effects,
                    "streams" => SourceKind::Streams,
                    _ => SourceKind::Timers,
                };
                self.expect("{");
                self.eat_seps();
                while !self.is("}") && !self.at_eof() {
                    let name = self.ident();
                    if !self.eat("=") {
                        self.expect(":");
                    }
                    let value = self.expr();
                    m.sources.push(MachineSource { kind, name, value });
                    self.eat_seps();
                }
                self.expect("}");
            } else if !self.state_body_entry(&mut m.body) {
                let t = self.peek();
                self.error(t.span, format!("unexpected `{}` in machine", self.text(t)));
                self.recover();
            }
        }
        self.expect("}");
        m
    }

    /// Parses one entry of a state body. Returns false if nothing matched.
    fn state_body_entry(&mut self, body: &mut StateBody) -> bool {
        if self.is("state") || self.is("final") || self.is("parallel") || self.is("initial") {
            let start = self.start();
            let mut is_final = false;
            let mut parallel = false;
            let mut is_initial = false;
            loop {
                if self.eat("final") {
                    is_final = true;
                } else if self.eat("parallel") {
                    parallel = true;
                } else if self.eat("initial") {
                    is_initial = true;
                } else {
                    break;
                }
            }
            self.expect("state");
            let name = self.ident();
            let fields = if self.is("(") { self.schema_fields("(", ")") } else { Vec::new() };
            let mut sub = StateBody::default();
            if self.is("{") && !self.peek().nl_before {
                self.bump();
                loop {
                    self.eat_seps();
                    if self.is("}") || self.at_eof() {
                        break;
                    }
                    if !self.state_body_entry(&mut sub) {
                        let t = self.peek();
                        self.error(t.span, format!("unexpected `{}` in state", self.text(t)));
                        self.recover();
                    }
                }
                self.expect("}");
            }
            body.states.push(StateDecl {
                name,
                is_initial,
                is_final,
                parallel,
                fields,
                body: sub,
                span: self.span_from(start),
            });
            return true;
        }
        if self.is("on") && self.peek_n(1).kind == TokenKind::Ident {
            let start = self.start();
            self.bump();
            let event = self.ident();
            let guard = if self.eat("if") { Some(self.expr_no_with()) } else { None };
            let transition = self.transition();
            body.ons.push(On { event, guard, transition, span: self.span_from(start) });
            return true;
        }
        if self.is("invoke") && self.peek_n(1).kind == TokenKind::Ident {
            let start = self.start();
            self.bump();
            let src = self.ident();
            let input = if self.is("(") && !self.peek().nl_before {
                self.bump();
                let e = self.expr();
                self.expect(")");
                Some(e)
            } else {
                None
            };
            let mut handlers = Vec::new();
            if self.is("{") && !self.peek().nl_before {
                self.bump();
                loop {
                    self.eat_seps();
                    if self.is("}") || self.at_eof() {
                        break;
                    }
                    let name = self.ident();
                    let tr = self.transition();
                    handlers.push((name, tr));
                }
                self.expect("}");
            }
            body.invokes.push(Invoke { src: InvokeSrc::Named(src), input, handlers, span: self.span_from(start) });
            return true;
        }
        if self.is("after") {
            let start = self.start();
            let kw = self.bump();
            let delay = self.expr_no_with();
            let tr = self.transition();
            body.invokes.push(Invoke {
                src: InvokeSrc::After(delay),
                input: None,
                handlers: vec![(Ident { name: "done".into(), span: kw.span }, tr)],
                span: self.span_from(start),
            });
            return true;
        }
        false
    }

    fn transition(&mut self) -> Transition {
        let start = self.start();
        if self.eat("update") {
            let owner = self.ident();
            let data = self.object_lit_expr();
            return Transition::Update { owner, data, span: self.span_from(start) };
        }
        self.expect("->");
        self.target_after_arrow(true)
    }

    fn target_after_arrow(&mut self, allow_stay: bool) -> Transition {
        let start = self.start();
        if allow_stay && self.is("stay") {
            let t = self.bump();
            return Transition::Stay(t.span);
        }
        let mut path = vec![self.ident()];
        while self.is(".") && !self.peek().nl_before {
            self.bump();
            path.push(self.ident());
        }
        let data = if self.is("{") && !self.peek().nl_before { Some(self.object_lit_expr()) } else { None };
        Transition::Target { path, data, span: self.span_from(start) }
    }

    fn object_lit_expr(&mut self) -> Expr {
        let start = self.start();
        if !self.is("{") {
            let t = self.peek();
            self.error(t.span, "expected `{`");
            return Expr { kind: ExprKind::Error, span: t.span };
        }
        let props = self.object_props();
        Expr { kind: ExprKind::Object(props), span: self.span_from(start) }
    }

    // ---------------------------------------------------------------------------------------
    // Components
    // ---------------------------------------------------------------------------------------

    fn component(&mut self) -> Component {
        self.expect("component");
        let name = self.ident();
        let mut members = Vec::new();
        self.expect("{");
        loop {
            self.eat_seps();
            if self.is("}") || self.at_eof() {
                break;
            }
            let start = self.start();
            let kind = if (self.is("prop") || self.is("let")) && self.peek_n(1).kind == TokenKind::Ident {
                let is_prop = self.is("prop");
                self.bump();
                let field = self.component_field();
                if is_prop { MemberKind::Prop(field) } else { MemberKind::Let(field) }
            } else if self.is("event") && self.peek_n(1).kind == TokenKind::Ident {
                self.bump();
                let vstart = self.start();
                let vname = self.ident();
                let fields =
                    if self.is("(") && !self.peek().nl_before { self.schema_fields("(", ")") } else { Vec::new() };
                MemberKind::Event(Variant { name: vname, fields, span: self.span_from(vstart) })
            } else if self.is("update") && self.peek_n(1).kind == TokenKind::Ident {
                self.bump();
                let message = self.ident();
                let fields =
                    if self.is("(") && !self.peek().nl_before { self.schema_fields("(", ")") } else { Vec::new() };
                let body = if self.eat("=>") {
                    UpdateBody::Expr(self.expr())
                } else {
                    let saved = std::mem::replace(&mut self.in_update, true);
                    let b = self.block();
                    self.in_update = saved;
                    UpdateBody::Block(b)
                };
                MemberKind::Update(UpdateArm { message, fields, body, span: self.span_from(start) })
            } else if self.is("view") && (self.is_n(1, "=>") || self.is_n(1, "=")) {
                self.bump();
                self.bump();
                MemberKind::View(self.expr())
            } else if self.is_ident() && self.is_n(1, "=") {
                let key = self.ident();
                self.bump();
                MemberKind::Option(key, self.expr())
            } else {
                let t = self.peek();
                self.error(t.span, format!("unexpected `{}` in component", self.text(t)));
                self.recover();
                continue;
            };
            members.push(Member { kind, span: self.span_from(start) });
        }
        self.expect("}");
        Component { name, members }
    }

    /// `name?: Type = default` in a component.
    fn component_field(&mut self) -> SchemaField {
        let start = self.start();
        let name = self.ident();
        let optional = self.eat("?");
        let value = if self.eat(":") {
            SchemaFieldValue::Type(self.ty())
        } else {
            let t = self.peek();
            self.error(t.span, format!("give `{}` a type: `{}: Type`", name.name, name.name));
            SchemaFieldValue::Type(Type {
                kind: TypeKind::Ref { name: vec![Ident { name: "unknown".into(), span: t.span }], args: None },
                span: t.span,
            })
        };
        let default = if self.eat("=") { Some(self.expr()) } else { None };
        SchemaField { name, optional, value, default, span: self.span_from(start) }
    }

    // ---------------------------------------------------------------------------------------
    // Statements
    // ---------------------------------------------------------------------------------------

    pub fn block(&mut self) -> Block {
        let start = self.start();
        self.expect("{");
        let mut stmts = Vec::new();
        loop {
            while self.eat(";") {}
            if self.is("}") || self.at_eof() {
                break;
            }
            let before = self.peek().span.start;
            stmts.push(self.stmt());
            self.end_stmt();
            if self.peek().span.start == before {
                self.bump();
            }
        }
        self.expect("}");
        Block { stmts, span: self.span_from(start) }
    }

    fn stmt(&mut self) -> Stmt {
        let start = self.start();
        let kind = self.stmt_kind();
        Stmt { kind, span: self.span_from(start) }
    }

    fn word_starts_expr(&mut self) -> bool {
        let t = self.peek_n(1);
        !t.nl_before
            && t.kind != TokenKind::Eof
            && !(t.kind == TokenKind::Punct
                && matches!(
                    self.text(t),
                    "(" | "." | "?." | "=" | ")" | "," | "}" | "]" | ";" | ":" | "=>" | "+=" | "-=" | "|>"
                ))
    }

    fn stmt_kind(&mut self) -> StmtKind {
        if self.is("const") || self.is("let") || self.is("var") {
            return StmtKind::Let(self.let_decl());
        }
        if self.is("function") && self.peek_n(1).kind == TokenKind::Ident {
            return StmtKind::Fn(self.fn_decl(false));
        }
        if self.is("effect") && self.is_n(1, "function") && self.peek_n(2).kind == TokenKind::Ident {
            self.bump();
            return StmtKind::Fn(self.fn_decl(true));
        }
        if self.is("return") {
            self.bump();
            let t = self.peek();
            if t.nl_before || t.kind == TokenKind::Eof || self.is("}") || self.is(";") {
                return StmtKind::Return(None);
            }
            return StmtKind::Return(Some(self.expr()));
        }
        if self.is("if") {
            return StmtKind::If(self.if_stmt());
        }
        if self.is("for") {
            self.bump();
            let bind = self.is("run") && !self.is_n(1, "of");
            if bind {
                let kw = self.bump();
                if !self.in_effect {
                    self.error(kw.span, "`for run` can only be used inside an effect body");
                }
            }
            let kind = if self.eat("let") {
                LetKind::Let
            } else {
                self.eat("const");
                LetKind::Const
            };
            let pattern = self.pattern();
            self.expect("of");
            let iter = self.expr();
            let body = if bind { self.effect_block() } else { self.block() };
            return StmtKind::For { kind, pattern, iter, body, bind };
        }
        if self.is("while") {
            self.bump();
            let cond = self.expr();
            let body = self.block();
            return StmtKind::While { cond, body };
        }
        if self.is("break") {
            self.bump();
            return StmtKind::Break;
        }
        if self.is("continue") {
            self.bump();
            return StmtKind::Continue;
        }
        if self.is("emit") && self.word_starts_expr() {
            let kw = self.bump();
            if !self.in_update {
                self.error(kw.span, "`emit` can only be used in a component `update`");
            }
            return StmtKind::Emit(self.expr());
        }
        for kw in ["fail", "defer", "throw"] {
            if self.is(kw) && self.word_starts_expr() {
                self.bump();
                let e = self.expr();
                return match kw {
                    "fail" => StmtKind::Fail(e),
                    "defer" => StmtKind::Defer(e),
                    _ => StmtKind::Throw(e),
                };
            }
        }
        if self.is("{") {
            return StmtKind::Block(self.block());
        }
        StmtKind::Expr(self.expr())
    }

    fn if_stmt(&mut self) -> IfStmt {
        self.expect("if");
        let cond = self.expr();
        let then = self.block();
        let else_ = if self.is("else") {
            self.bump();
            let start = self.start();
            if self.is("if") {
                let s = self.if_stmt();
                Some(Box::new(Stmt { kind: StmtKind::If(s), span: self.span_from(start) }))
            } else {
                let b = self.block();
                Some(Box::new(Stmt { kind: StmtKind::Block(b), span: self.span_from(start) }))
            }
        } else {
            None
        };
        IfStmt { cond, then, else_ }
    }

    // ---------------------------------------------------------------------------------------
    // Patterns and parameters
    // ---------------------------------------------------------------------------------------

    fn pattern(&mut self) -> Pattern {
        let start = self.start();
        if self.eat("{") {
            let mut props = Vec::new();
            let mut rest = None;
            while !self.is("}") && !self.at_eof() {
                if self.eat("...") {
                    rest = Some(self.ident());
                } else {
                    let key = self.ident();
                    let value = if self.eat(":") { Some(self.pattern()) } else { None };
                    let default = if self.eat("=") { Some(self.expr_no_with()) } else { None };
                    props.push(PatternProp { key, value, default });
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.expect("}");
            return Pattern { kind: PatternKind::Object { props, rest }, span: self.span_from(start) };
        }
        if self.eat("[") {
            let mut elems = Vec::new();
            let mut rest = None;
            while !self.is("]") && !self.at_eof() {
                if self.is(",") {
                    self.bump();
                    elems.push(None);
                    continue;
                }
                if self.eat("...") {
                    rest = Some(Box::new(self.pattern()));
                } else {
                    let p = self.pattern();
                    if self.eat("=") {
                        let default = self.expr_no_with();
                        let span = p.span.to(default.span);
                        elems.push(Some(Pattern {
                            kind: PatternKind::Default { pattern: Box::new(p), default: Box::new(default) },
                            span,
                        }));
                    } else {
                        elems.push(Some(p));
                    }
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.expect("]");
            return Pattern { kind: PatternKind::Array { elems, rest }, span: self.span_from(start) };
        }
        let id = self.ident();
        Pattern { span: id.span, kind: PatternKind::Ident(id) }
    }

    fn params(&mut self) -> Vec<Param> {
        self.expect("(");
        let mut params = Vec::new();
        while !self.is(")") && !self.at_eof() {
            let start = self.start();
            let rest = self.eat("...");
            let pattern = self.pattern();
            let optional = self.eat("?");
            let ty = if self.eat(":") { Some(self.ty()) } else { None };
            let default = if self.eat("=") { Some(self.expr_no_with()) } else { None };
            params.push(Param { pattern, optional, rest, ty, default, span: self.span_from(start) });
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")");
        params
    }

    fn type_params(&mut self) -> Vec<TypeParam> {
        let mut out = Vec::new();
        if !self.is("<") {
            return out;
        }
        self.bump();
        while !self.is(">") && !self.at_eof() {
            let is_const = self.is("const") && self.peek_n(1).kind == TokenKind::Ident;
            if is_const {
                self.bump();
            }
            let name = self.ident();
            let constraint = if self.eat("extends") { Some(self.ty()) } else { None };
            let default = if self.eat("=") { Some(self.ty()) } else { None };
            out.push(TypeParam { name, constraint, default, is_const });
            if !self.eat(",") {
                break;
            }
        }
        self.expect(">");
        out
    }

    // ---------------------------------------------------------------------------------------
    // Expressions
    // ---------------------------------------------------------------------------------------

    pub fn expr(&mut self) -> Expr {
        let start = self.start();
        if self.at_run() {
            return self.run_expr(false);
        }
        if self.at_use() {
            return self.use_expr(false);
        }
        let mut left = self.assign_expr();
        while self.is("with") {
            self.bump();
            let layer = self.assign_expr();
            left = Expr {
                kind: ExprKind::With { expr: Box::new(left), layer: Box::new(layer) },
                span: self.span_from(start),
            };
        }
        left
    }

    /// An expression that stops before `with`.
    fn expr_no_with(&mut self) -> Expr {
        if self.at_run() {
            return self.run_expr(true);
        }
        if self.at_use() {
            return self.use_expr(true);
        }
        self.assign_expr()
    }

    fn assign_expr(&mut self) -> Expr {
        let start = self.start();
        if let Some(f) = self.try_arrow(false, start) {
            return Expr { kind: ExprKind::Func(Box::new(f)), span: self.span_from(start) };
        }
        let left = self.cond_expr();
        let t = self.peek();
        if t.kind == TokenKind::Punct && ASSIGN_OPS.contains(&self.text(t)) {
            let op_tok = self.bump();
            let op = self.text(op_tok).to_string();
            let value = self.expr();
            return Expr {
                kind: ExprKind::Assign { op, target: Box::new(left), value: Box::new(value) },
                span: self.span_from(start),
            };
        }
        left
    }

    fn try_arrow(&mut self, effect: bool, start: u32) -> Option<Func> {
        if self.is_ident() && self.is_n(1, "=>") && !self.peek_n(1).nl_before {
            let id = self.ident();
            self.bump();
            let pattern = Pattern { span: id.span, kind: PatternKind::Ident(id) };
            let param = Param { span: pattern.span, pattern, optional: false, rest: false, ty: None, default: None };
            let body = self.arrow_body(effect);
            return Some(Func {
                effect,
                arrow: true,
                type_params: Vec::new(),
                params: vec![param],
                ret: None,
                body,
                span: self.span_from(start),
            });
        }
        if !self.is("(") && !self.is("<") {
            return None;
        }
        // Speculatively parse a parameter list followed by `=>`.
        let cp = self.checkpoint();
        let diags = self.diags.len();
        let type_params = if self.is("<") { self.type_params() } else { Vec::new() };
        let params = self.params();
        let ret = if self.is(":") {
            self.bump();
            Some(self.ty())
        } else {
            None
        };
        if self.diags.len() != diags || !self.is("=>") || self.peek().nl_before {
            self.restore(cp);
            return None;
        }
        self.bump();
        let body = self.arrow_body(effect);
        Some(Func { effect, arrow: true, type_params, params, ret, body, span: self.span_from(start) })
    }

    fn arrow_body(&mut self, effect: bool) -> FuncBody {
        let saved = std::mem::replace(&mut self.in_effect, effect);
        let saved_update = std::mem::replace(&mut self.in_update, false);
        let body = if self.is("{") { FuncBody::Block(self.block()) } else { FuncBody::Expr(Box::new(self.expr())) };
        self.in_effect = saved;
        self.in_update = saved_update;
        body
    }

    fn cond_expr(&mut self) -> Expr {
        let start = self.start();
        let cond = self.pipe_expr();
        if self.is("?") {
            self.bump();
            let then = self.expr_no_with();
            self.expect(":");
            let else_ = self.expr_no_with();
            return Expr {
                kind: ExprKind::Cond { cond: Box::new(cond), then: Box::new(then), else_: Box::new(else_) },
                span: self.span_from(start),
            };
        }
        cond
    }

    fn pipe_expr(&mut self) -> Expr {
        let start = self.start();
        let mut left = self.binary(0);
        loop {
            if self.is("|>") {
                let mut parts = match left.kind {
                    ExprKind::Pipe(parts) => parts,
                    _ => vec![left],
                };
                self.bump();
                parts.push(self.binary(0));
                left = Expr { kind: ExprKind::Pipe(parts), span: self.span_from(start) };
            } else if self.is("catch")
                && (self.is_n(1, "{") || self.peek_n(1).kind == TokenKind::Ident || self.is_n(1, "("))
            {
                self.bump();
                let handler = self.catch_handler();
                left = Expr { kind: ExprKind::Catch { expr: Box::new(left), handler }, span: self.span_from(start) };
            } else {
                return left;
            }
        }
    }

    fn catch_handler(&mut self) -> CatchHandler {
        if !self.is("{") {
            let binding = if self.eat("(") {
                let p = self.pattern();
                self.expect(")");
                p
            } else {
                self.pattern()
            };
            self.expect("=>");
            let body = self.expr_no_with();
            return CatchHandler::All { binding, body: Box::new(body) };
        }
        self.bump();
        let mut arms = Vec::new();
        loop {
            self.eat_seps();
            if self.is("}") || self.at_eof() {
                break;
            }
            let start = self.start();
            let tag = self.ident();
            let binding = if self.eat("(") {
                let p = self.pattern();
                self.expect(")");
                Some(p)
            } else {
                None
            };
            self.expect("=>");
            let body = self.expr();
            arms.push(CatchArm { tag, binding, body, span: self.span_from(start) });
        }
        self.expect("}");
        CatchHandler::Tags(arms)
    }

    fn bin_prec(&mut self) -> Option<(String, u8)> {
        let t = self.peek();
        if !matches!(t.kind, TokenKind::Punct | TokenKind::Ident) {
            return None;
        }
        let mut text = self.text(t).to_string();
        // `>` is lexed alone so that nested type arguments close; recombine shifts here.
        if text == ">" {
            let t1 = self.peek_n(1);
            if self.text(t1) == ">" && t1.span.start == t.span.end {
                let t2 = self.peek_n(2);
                text = if self.text(t2) == ">" && t2.span.start == t1.span.end { ">>>".into() } else { ">>".into() };
            }
        }
        if t.nl_before && text == "<" {
            return None;
        }
        if t.kind == TokenKind::Ident && t.nl_before {
            return None;
        }
        BIN_PREC.iter().find(|(op, _)| *op == text).map(|&(op, p)| (op.to_string(), p))
    }

    fn binary(&mut self, min_prec: u8) -> Expr {
        let start = self.start();
        let mut left = self.unary();
        while let Some((op, prec)) = self.bin_prec() {
            if prec <= min_prec && !(op == "**" && prec == min_prec) {
                break;
            }
            let tokens = match op.as_str() {
                ">>" => 2,
                ">>>" => 3,
                _ => 1,
            };
            for _ in 0..tokens {
                self.bump();
            }
            if op == "as" || op == "satisfies" {
                let ty = if op == "as" && self.is("const") {
                    let t = self.bump();
                    Type {
                        kind: TypeKind::Ref { name: vec![Ident { name: "const".into(), span: t.span }], args: None },
                        span: t.span,
                    }
                } else {
                    self.ty()
                };
                let kind = if op == "as" {
                    ExprKind::As { expr: Box::new(left), ty }
                } else {
                    ExprKind::Satisfies { expr: Box::new(left), ty }
                };
                left = Expr { kind, span: self.span_from(start) };
                continue;
            }
            let right = self.binary(prec);
            left = Expr {
                kind: ExprKind::Binary { op, left: Box::new(left), right: Box::new(right) },
                span: self.span_from(start),
            };
        }
        left
    }

    fn unary(&mut self) -> Expr {
        let start = self.start();
        let t = self.peek();
        let text = self.text(t);
        let is_op = (t.kind == TokenKind::Punct && matches!(text, "!" | "-" | "+" | "~"))
            || (t.kind == TokenKind::Ident && matches!(text, "typeof" | "void" | "delete") && self.word_starts_expr());
        if is_op {
            self.bump();
            let expr = self.unary();
            return Expr {
                kind: ExprKind::Unary { op: text.to_string(), expr: Box::new(expr) },
                span: self.span_from(start),
            };
        }
        if t.kind == TokenKind::Punct && matches!(text, "++" | "--") {
            self.bump();
            let expr = self.unary();
            return Expr {
                kind: ExprKind::Update { op: text.to_string(), prefix: true, expr: Box::new(expr) },
                span: self.span_from(start),
            };
        }
        self.postfix_expr(true)
    }

    fn args(&mut self) -> Vec<ArrayElem> {
        self.expect("(");
        let mut args = Vec::new();
        while !self.is(")") && !self.at_eof() {
            if self.eat("...") {
                args.push(ArrayElem::Spread(self.expr()));
            } else {
                args.push(ArrayElem::Expr(self.expr()));
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")");
        args
    }

    /// Tries `<Type, ...>(` right after a callee, without whitespace before `<`.
    fn try_type_args(&mut self) -> Option<Vec<Type>> {
        let t = self.peek();
        if !self.is("<") || t.span.start != self.prev_end {
            return None;
        }
        let cp = self.checkpoint();
        let diags = self.diags.len();
        self.bump();
        let mut args = Vec::new();
        while !self.is(">") && !self.at_eof() {
            args.push(self.ty());
            if !self.eat(",") {
                break;
            }
        }
        if self.diags.len() == diags && self.is(">") {
            self.bump();
            if self.is("(") && !self.peek().nl_before {
                return Some(args);
            }
        }
        self.restore(cp);
        None
    }

    fn postfix_expr(&mut self, allow_call: bool) -> Expr {
        let start = self.start();
        let mut e = if self.is("new") && self.word_starts_expr() {
            self.bump();
            let callee = self.postfix_expr(false);
            let type_args = self.try_type_args();
            let args = if self.is("(") && !self.peek().nl_before { self.args() } else { Vec::new() };
            Expr { kind: ExprKind::New { callee: Box::new(callee), type_args, args }, span: self.span_from(start) }
        } else {
            self.primary()
        };
        loop {
            let t = self.peek();
            let text = if matches!(t.kind, TokenKind::Punct) { self.text(t) } else { "" };
            match text {
                "." => {
                    self.bump();
                    let name = self.ident();
                    e = Expr {
                        kind: ExprKind::Member { object: Box::new(e), name, optional: false },
                        span: self.span_from(start),
                    };
                }
                "?." => {
                    self.bump();
                    if self.is("(") {
                        let args = self.args();
                        e = Expr {
                            kind: ExprKind::Call { callee: Box::new(e), type_args: None, args, optional: true },
                            span: self.span_from(start),
                        };
                    } else if self.eat("[") {
                        let index = self.expr();
                        self.expect("]");
                        e = Expr {
                            kind: ExprKind::Index { object: Box::new(e), index: Box::new(index), optional: true },
                            span: self.span_from(start),
                        };
                    } else {
                        let name = self.ident();
                        e = Expr {
                            kind: ExprKind::Member { object: Box::new(e), name, optional: true },
                            span: self.span_from(start),
                        };
                    }
                }
                "(" if allow_call && !t.nl_before => {
                    let args = self.args();
                    e = Expr {
                        kind: ExprKind::Call { callee: Box::new(e), type_args: None, args, optional: false },
                        span: self.span_from(start),
                    };
                }
                "<" if allow_call => {
                    let Some(type_args) = self.try_type_args() else { break };
                    let args = self.args();
                    e = Expr {
                        kind: ExprKind::Call { callee: Box::new(e), type_args: Some(type_args), args, optional: false },
                        span: self.span_from(start),
                    };
                }
                "[" if !t.nl_before => {
                    self.bump();
                    let index = self.expr();
                    self.expect("]");
                    e = Expr {
                        kind: ExprKind::Index { object: Box::new(e), index: Box::new(index), optional: false },
                        span: self.span_from(start),
                    };
                }
                "!" if !t.nl_before && t.span.start == self.prev_end && !self.is_n(1, "=") => {
                    self.bump();
                    e = Expr { kind: ExprKind::NonNull(Box::new(e)), span: self.span_from(start) };
                }
                "++" | "--" if !t.nl_before => {
                    self.bump();
                    e = Expr {
                        kind: ExprKind::Update { op: text.to_string(), prefix: false, expr: Box::new(e) },
                        span: self.span_from(start),
                    };
                }
                _ => {
                    if allow_call && matches!(t.kind, TokenKind::TemplateFull | TokenKind::TemplateHead) && !t.nl_before
                    {
                        let span = t.span;
                        self.error(span, "tagged templates are not supported");
                    }
                    break;
                }
            }
        }
        e
    }

    fn primary(&mut self) -> Expr {
        let start = self.start();
        let t = self.peek();
        let text = self.text(t);
        match t.kind {
            TokenKind::Number | TokenKind::String => {
                self.bump();
                return Expr { kind: ExprKind::Lit(text.to_string()), span: t.span };
            }
            TokenKind::TemplateFull => {
                self.bump();
                return Expr { kind: ExprKind::Template(vec![TemplatePart::Str(t.span)]), span: t.span };
            }
            TokenKind::TemplateHead => return self.template(),
            TokenKind::Ident => match text {
                "true" | "false" | "null" => {
                    self.bump();
                    return Expr { kind: ExprKind::Lit(text.to_string()), span: t.span };
                }
                "effect" if self.is_n(1, "{") && !self.peek_n(1).nl_before => {
                    self.bump();
                    let b = self.effect_block();
                    return Expr { kind: ExprKind::EffectBlock(b), span: self.span_from(start) };
                }
                "effect" if self.is_n(1, "function") => {
                    self.bump();
                    self.bump();
                    let f = self.func_rest(true, start);
                    return Expr { kind: ExprKind::Func(Box::new(f)), span: self.span_from(start) };
                }
                "effect"
                    if !self.peek_n(1).nl_before
                        && (self.is_n(1, "(")
                            || self.is_n(1, "<")
                            || (self.peek_n(1).kind == TokenKind::Ident && self.is_n(2, "=>"))) =>
                {
                    self.bump();
                    match self.try_arrow(true, start) {
                        Some(f) => return Expr { kind: ExprKind::Func(Box::new(f)), span: self.span_from(start) },
                        None => {
                            let t = self.peek();
                            self.error(
                                t.span,
                                "expected an arrow function after `effect`, such as `effect (x) => ...`",
                            );
                            return Expr { kind: ExprKind::Error, span: t.span };
                        }
                    }
                }
                "run" if self.in_effect || self.in_update => {
                    // `run` inside a larger expression: `(run a) + (run b)`.
                    let span = t.span;
                    self.error(span, "wrap `run ...` in parentheses when it is part of a larger expression");
                    return self.run_expr(true);
                }
                "use" if self.at_use() => {
                    // `use` inside a larger expression: `(use A).b`.
                    let span = t.span;
                    self.error(span, "wrap `use ...` in parentheses when it is part of a larger expression");
                    return self.use_expr(true);
                }
                "function" if self.is_n(1, "(") || self.is_n(1, "<") => {
                    self.bump();
                    let f = self.func_rest(false, start);
                    return Expr { kind: ExprKind::Func(Box::new(f)), span: self.span_from(start) };
                }
                "if" => return self.if_expr(),
                "match" if self.word_starts_expr() && !self.is_n(1, "{") => return self.match_expr(),
                _ => {
                    self.bump();
                    return Expr { kind: ExprKind::Ident(text.to_string()), span: t.span };
                }
            },
            TokenKind::Punct => match text {
                "(" => {
                    self.bump();
                    let inner = self.expr();
                    self.expect(")");
                    return Expr { kind: ExprKind::Paren(Box::new(inner)), span: self.span_from(start) };
                }
                "[" => {
                    self.bump();
                    let mut elems = Vec::new();
                    while !self.is("]") && !self.at_eof() {
                        if self.is(",") {
                            self.bump();
                            elems.push(ArrayElem::Hole);
                            continue;
                        }
                        if self.eat("...") {
                            elems.push(ArrayElem::Spread(self.expr()));
                        } else {
                            elems.push(ArrayElem::Expr(self.expr()));
                        }
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.expect("]");
                    return Expr { kind: ExprKind::Array(elems), span: self.span_from(start) };
                }
                "{" => {
                    let props = self.object_props();
                    return Expr { kind: ExprKind::Object(props), span: self.span_from(start) };
                }
                "<" if self.peek_n(1).kind == TokenKind::Ident && self.peek_n(1).span.start == t.span.end => {
                    let el = self.element();
                    return Expr { span: el.span, kind: ExprKind::Markup(Box::new(el)) };
                }
                _ => {}
            },
            _ => {}
        }
        let msg = if t.kind == TokenKind::Eof {
            "expected an expression, found end of file".to_string()
        } else {
            format!("expected an expression, found `{text}`")
        };
        self.error(t.span, msg);
        if t.kind != TokenKind::Eof && !matches!(text, "}" | ")" | "]") {
            self.bump();
        }
        Expr { kind: ExprKind::Error, span: t.span }
    }

    fn template(&mut self) -> Expr {
        let start = self.start();
        let mut parts = Vec::new();
        let head = self.bump();
        parts.push(TemplatePart::Str(head.span));
        loop {
            let e = self.expr();
            parts.push(TemplatePart::Expr(e));
            let t = self.peek();
            match t.kind {
                TokenKind::TemplateMiddle => {
                    self.bump();
                    parts.push(TemplatePart::Str(t.span));
                }
                TokenKind::TemplateTail => {
                    self.bump();
                    parts.push(TemplatePart::Str(t.span));
                    break;
                }
                _ => {
                    self.error(t.span, "expected `}` to close the template substitution");
                    break;
                }
            }
        }
        Expr { kind: ExprKind::Template(parts), span: self.span_from(start) }
    }

    fn object_props(&mut self) -> Vec<Prop> {
        self.expect("{");
        let mut props = Vec::new();
        while !self.is("}") && !self.at_eof() {
            let before = self.peek().span.start;
            if self.eat("...") {
                props.push(Prop::Spread(self.expr()));
            } else {
                let effect = self.is("effect") && (self.peek_n(1).kind == TokenKind::Ident) && self.is_n(2, "(");
                let fstart = self.start();
                if effect {
                    self.bump();
                }
                let key = self.prop_key();
                if self.is("(") || self.is("<") {
                    let func = self.func_rest(effect, fstart);
                    props.push(Prop::Method { key, func });
                } else if self.eat(":") {
                    props.push(Prop::KeyValue { key, value: self.expr() });
                } else if let PropKey::Ident(id) = key {
                    props.push(Prop::Shorthand(id));
                } else {
                    let t = self.peek();
                    self.error(t.span, "expected `:`");
                }
            }
            if self.peek().span.start == before {
                // Nothing was consumed: stop rather than loop on the same token.
                break;
            }
            if !self.eat(",") {
                if !self.is("}") && self.peek().nl_before {
                    continue;
                }
                break;
            }
        }
        self.expect("}");
        props
    }

    fn prop_key(&mut self) -> PropKey {
        let t = self.peek();
        match t.kind {
            TokenKind::String => PropKey::Str(self.string_lit()),
            TokenKind::Number => {
                self.bump();
                PropKey::Num(self.text(t).to_string(), t.span)
            }
            _ if self.is("[") => {
                self.bump();
                let e = self.expr();
                self.expect("]");
                PropKey::Computed(Box::new(e))
            }
            _ => PropKey::Ident(self.ident()),
        }
    }

    fn if_expr(&mut self) -> Expr {
        let start = self.start();
        self.expect("if");
        let cond = self.expr();
        let then = self.branch_expr();
        let else_ = if self.eat("else") {
            if self.is("if") { self.if_expr() } else { self.branch_expr() }
        } else {
            let t = self.peek();
            self.error(t.span, "an `if` expression needs an `else` branch");
            Expr { kind: ExprKind::Error, span: t.span }
        };
        Expr {
            kind: ExprKind::If { cond: Box::new(cond), then: Box::new(then), else_: Box::new(else_) },
            span: self.span_from(start),
        }
    }

    /// `{ expr }` in an `if` expression.
    fn branch_expr(&mut self) -> Expr {
        self.expect("{");
        let e = self.expr();
        self.eat(";");
        self.expect("}");
        e
    }

    fn match_expr(&mut self) -> Expr {
        let start = self.start();
        self.expect("match");
        let scrutinee = self.expr_no_with();
        self.expect("{");
        let mut arms = Vec::new();
        loop {
            self.eat_seps();
            if self.is("}") || self.at_eof() {
                break;
            }
            let astart = self.start();
            let t = self.peek();
            let pattern = match t.kind {
                TokenKind::String | TokenKind::Number => {
                    self.bump();
                    MatchPattern::Lit(self.text(t).to_string(), t.span)
                }
                TokenKind::Ident if matches!(self.text(t), "true" | "false" | "null" | "undefined") => {
                    self.bump();
                    MatchPattern::Lit(self.text(t).to_string(), t.span)
                }
                TokenKind::Ident if self.text(t) == "_" => {
                    self.bump();
                    MatchPattern::Wildcard(t.span)
                }
                _ => {
                    let tag = self.ident();
                    let binding = if self.eat("(") {
                        let p = self.pattern();
                        self.expect(")");
                        Some(p)
                    } else {
                        None
                    };
                    MatchPattern::Tag { tag, binding }
                }
            };
            if self.is("if") {
                let t = self.peek();
                self.error(t.span, "match guards are not supported; use a nested `if` expression");
            }
            self.expect("=>");
            let body = self.expr();
            arms.push(MatchArm { pattern, body, span: self.span_from(astart) });
        }
        self.expect("}");
        Expr { kind: ExprKind::Match { expr: Box::new(scrutinee), arms }, span: self.span_from(start) }
    }

    // ---------------------------------------------------------------------------------------
    // Markup
    // ---------------------------------------------------------------------------------------

    fn markup_name(&mut self) -> Vec<Ident> {
        let mut name = vec![self.ident()];
        while self.is(".") && self.peek().span.start == self.prev_end {
            self.bump();
            name.push(self.ident());
        }
        name
    }

    /// An attribute name such as `class`, `aria-label`, `on:click` or `on:keydown|preventDefault`.
    fn attr_name(&mut self) -> Ident {
        let first = self.ident();
        let mut name = first.name;
        let mut span = first.span;
        loop {
            let t = self.peek();
            let t1 = self.peek_n(1);
            if t.span.start == self.prev_end
                && t.kind == TokenKind::Punct
                && matches!(self.text(t), "-" | ":" | "|")
                && t1.kind == TokenKind::Ident
                && t1.span.start == t.span.end
            {
                self.bump();
                let id = self.ident();
                name.push_str(self.text(t));
                name.push_str(&id.name);
                span = span.to(id.span);
            } else {
                break;
            }
        }
        Ident { name, span }
    }

    fn element(&mut self) -> Element {
        let start = self.start();
        self.expect("<");
        let name = self.markup_name();
        let mut attrs = Vec::new();
        loop {
            if self.is("/") || self.is(">") || self.at_eof() {
                break;
            }
            if self.is("{") && self.is_n(1, "...") {
                self.bump();
                self.bump();
                let e = self.expr();
                self.expect("}");
                attrs.push(Attr::Spread(e));
                continue;
            }
            if !self.is_ident() {
                let t = self.peek();
                self.error(t.span, "expected an attribute name");
                self.bump();
                continue;
            }
            let astart = self.start();
            let aname = self.attr_name();
            let value = if self.eat("=") {
                if self.peek().kind == TokenKind::String {
                    Some(AttrValue::Str(self.string_lit()))
                } else {
                    self.expect("{");
                    let e = self.expr();
                    self.expect("}");
                    Some(AttrValue::Expr(e))
                }
            } else {
                None
            };
            attrs.push(Attr::Named { name: aname, value, span: self.span_from(astart) });
        }
        if self.eat("/") {
            self.expect(">");
            return Element { name, attrs, children: Vec::new(), self_closing: true, span: self.span_from(start) };
        }
        self.expect(">");
        let children = self.markup_children(&name);
        Element { name, attrs, children, self_closing: false, span: self.span_from(start) }
    }

    fn markup_children(&mut self, open: &[Ident]) -> Vec<Child> {
        let mut children = Vec::new();
        loop {
            // Scan raw text up to the next `<` or `{`.
            self.reset_after_last();
            let from = self.prev_end as usize;
            let rest = &self.src[from..];
            let len = rest.find(['<', '{']).unwrap_or(rest.len());
            if len > 0 {
                let raw = &rest[..len];
                let span = Span::new(from as u32, (from + len) as u32);
                if !raw.trim().is_empty() {
                    children.push(Child::Text(raw.to_string(), span));
                }
                self.prev_end = span.end;
                self.reset_after_last();
            }
            if self.at_eof() {
                let span = Span::new(self.prev_end, self.prev_end);
                self.error(span, format!("unclosed element `<{}>`", join_name(open)));
                return children;
            }
            if self.is("{") {
                self.bump();
                if self.is("}") {
                    self.bump();
                    continue;
                }
                let e = self.expr();
                self.expect("}");
                children.push(Child::Expr(e));
                continue;
            }
            if self.is("<") && self.is_n(1, "/") {
                self.bump();
                self.bump();
                let close = self.markup_name();
                if join_name(&close) != join_name(open) {
                    let span = close[0].span.to(close.last().unwrap().span);
                    self.error(span, format!("expected `</{}>`", join_name(open)));
                }
                self.expect(">");
                return children;
            }
            if self.is("<") {
                children.push(Child::Element(self.element()));
                continue;
            }
            let t = self.bump();
            self.error(t.span, "unexpected token in markup");
        }
    }

    // ---------------------------------------------------------------------------------------
    // Types
    // ---------------------------------------------------------------------------------------

    pub fn ty(&mut self) -> Type {
        let start = self.start();
        self.eat("|");
        let first = self.intersection_ty();
        if !self.is("|") {
            return first;
        }
        let mut members = vec![first];
        while self.eat("|") {
            members.push(self.intersection_ty());
        }
        Type { kind: TypeKind::Union(members), span: self.span_from(start) }
    }

    fn intersection_ty(&mut self) -> Type {
        let start = self.start();
        self.eat("&");
        let first = self.postfix_ty();
        if !self.is("&") {
            return first;
        }
        let mut members = vec![first];
        while self.eat("&") {
            members.push(self.postfix_ty());
        }
        Type { kind: TypeKind::Intersection(members), span: self.span_from(start) }
    }

    fn postfix_ty(&mut self) -> Type {
        let start = self.start();
        let mut t = self.primary_ty();
        while self.is("[") && !self.peek().nl_before {
            self.bump();
            if self.eat("]") {
                t = Type { kind: TypeKind::Array(Box::new(t)), span: self.span_from(start) };
            } else {
                let index = self.ty();
                self.expect("]");
                t = Type {
                    kind: TypeKind::Indexed { object: Box::new(t), index: Box::new(index) },
                    span: self.span_from(start),
                };
            }
        }
        t
    }

    fn dotted(&mut self) -> Vec<Ident> {
        let mut name = vec![self.ident()];
        while self.is(".") {
            self.bump();
            name.push(self.ident());
        }
        name
    }

    fn primary_ty(&mut self) -> Type {
        let start = self.start();
        let t = self.peek();
        let text = self.text(t);
        match t.kind {
            TokenKind::String | TokenKind::Number => {
                self.bump();
                return Type { kind: TypeKind::Lit(text.to_string()), span: t.span };
            }
            TokenKind::TemplateFull => {
                self.bump();
                return Type { kind: TypeKind::Template(text.to_string()), span: t.span };
            }
            TokenKind::Ident => match text {
                "true" | "false" | "null" => {
                    self.bump();
                    return Type { kind: TypeKind::Lit(text.to_string()), span: t.span };
                }
                "typeof" => {
                    self.bump();
                    let name = self.dotted();
                    return Type { kind: TypeKind::Typeof(name), span: self.span_from(start) };
                }
                "keyof" => {
                    self.bump();
                    let inner = self.postfix_ty();
                    return Type { kind: TypeKind::Keyof(Box::new(inner)), span: self.span_from(start) };
                }
                "readonly" => {
                    self.bump();
                    let inner = self.postfix_ty();
                    return Type { kind: TypeKind::Readonly(Box::new(inner)), span: self.span_from(start) };
                }
                _ => {
                    let name = self.dotted();
                    let args = if self.is("<") && !self.peek().nl_before {
                        self.bump();
                        let mut args = Vec::new();
                        while !self.is(">") && !self.at_eof() {
                            args.push(self.ty());
                            if !self.eat(",") {
                                break;
                            }
                        }
                        self.expect(">");
                        Some(args)
                    } else {
                        None
                    };
                    return Type { kind: TypeKind::Ref { name, args }, span: self.span_from(start) };
                }
            },
            TokenKind::Punct => match text {
                "-" if self.peek_n(1).kind == TokenKind::Number => {
                    self.bump();
                    let n = self.bump();
                    let s = Span::new(t.span.start, n.span.end);
                    return Type { kind: TypeKind::Lit(s.text(self.src).to_string()), span: s };
                }
                "{" => {
                    let members = self.type_members();
                    return Type { kind: TypeKind::Object(members), span: self.span_from(start) };
                }
                "[" => {
                    self.bump();
                    let mut elems = Vec::new();
                    while !self.is("]") && !self.at_eof() {
                        let rest = self.eat("...");
                        let name = if self.is_ident() && (self.is_n(1, ":") || (self.is_n(1, "?") && self.is_n(2, ":")))
                        {
                            Some(self.ident())
                        } else {
                            None
                        };
                        let mut optional = false;
                        if name.is_some() {
                            optional = self.eat("?");
                            self.expect(":");
                        }
                        let ty = self.ty();
                        if name.is_none() && self.eat("?") {
                            optional = true;
                        }
                        elems.push(TupleElem { name, optional, rest, ty });
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.expect("]");
                    return Type { kind: TypeKind::Tuple(elems), span: self.span_from(start) };
                }
                "(" | "<" => {
                    let cp = self.checkpoint();
                    let diags = self.diags.len();
                    let type_params = self.type_params();
                    let params = self.params();
                    if self.diags.len() == diags && self.is("=>") {
                        self.bump();
                        let ret = self.ty();
                        return Type {
                            kind: TypeKind::Fn { type_params, params, ret: Box::new(ret) },
                            span: self.span_from(start),
                        };
                    }
                    self.restore(cp);
                    if self.eat("(") {
                        let inner = self.ty();
                        self.expect(")");
                        return Type { kind: TypeKind::Paren(Box::new(inner)), span: self.span_from(start) };
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.error(t.span, format!("expected a type, found `{text}`"));
        if t.kind != TokenKind::Eof && !matches!(text, "}" | ")" | "]" | ">" | ",") {
            self.bump();
        }
        Type {
            kind: TypeKind::Ref { name: vec![Ident { name: "unknown".into(), span: t.span }], args: None },
            span: t.span,
        }
    }

    fn type_members(&mut self) -> Vec<TypeMember> {
        self.expect("{");
        let mut members = Vec::new();
        self.eat_seps();
        while !self.is("}") && !self.at_eof() {
            let start = self.start();
            let readonly = self.is("readonly") && !self.is_n(1, ":") && !self.is_n(1, "?") && !self.is_n(1, "(");
            if readonly {
                self.bump();
            }
            if self.is("[") && self.peek_n(1).kind == TokenKind::Ident && self.is_n(2, ":") {
                let bstart = self.start();
                self.bump();
                let param = self.ident();
                self.expect(":");
                let key = self.ty();
                self.expect("]");
                self.expect(":");
                let value = self.ty();
                let span = self.span_from(bstart);
                members.push(TypeMember {
                    name: PropKey::Ident(Ident { name: String::new(), span }),
                    readonly,
                    optional: false,
                    kind: TypeMemberKind::Index { param, key, value },
                    span: self.span_from(start),
                });
            } else {
                let name = self.prop_key();
                let optional = self.eat("?");
                let kind = if self.is("(") || self.is("<") {
                    let type_params = self.type_params();
                    let params = self.params();
                    let ret = if self.eat(":") { Some(self.ty()) } else { None };
                    TypeMemberKind::Method { type_params, params, ret }
                } else {
                    self.expect(":");
                    TypeMemberKind::Prop(self.ty())
                };
                members.push(TypeMember { name, readonly, optional, kind, span: self.span_from(start) });
            }
            let before = self.peek().span.start;
            self.eat_seps();
            if !self.is("}") && self.peek().span.start == before && !self.peek().nl_before {
                let t = self.peek();
                self.error(t.span, "expected `,`, `;`, a new line or `}`");
                self.recover();
            }
        }
        self.expect("}");
        members
    }
}

fn join_name(name: &[Ident]) -> String {
    name.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join(".")
}
