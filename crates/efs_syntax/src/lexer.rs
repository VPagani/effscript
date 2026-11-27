//! A re-positionable lexer. The parser drives it lazily so that it can switch
//! into markup-text mode, where ordinary tokenization does not apply.

use std::collections::BTreeMap;

use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Ident,
    Number,
    String,
    /// A template literal without substitutions: `` `abc` ``.
    TemplateFull,
    /// `` `abc${ ``
    TemplateHead,
    /// `}abc${`
    TemplateMiddle,
    /// `` }abc` ``
    TemplateTail,
    Punct,
    Error,
    Eof,
}

#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// Whether a line break separates this token from the previous one.
    pub nl_before: bool,
    pub error: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub span: Span,
    pub block: bool,
    /// Whether the comment is the first thing on its line.
    pub own_line: bool,
}

#[derive(Clone, Debug)]
pub struct Lexer {
    pub pos: usize,
    /// One entry per open `{`: `true` when it opened a template substitution.
    braces: Vec<bool>,
}

const PUNCTS: &[&str] = &[
    "...", "===", "!==", "**=", "<<=", "&&=", "||=", "??=", "=>", "==", "!=", "<=", ">=", "&&", "||", "??", "?.", "++",
    "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "**", "<<", "|>", "->", "{", "}", "(", ")", "[", "]", ";",
    ",", "<", ">", "+", "-", "*", "/", "%", "&", "|", "^", "!", "~", "?", ":", "=", ".", "@", "#",
];

pub fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

pub fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

impl Lexer {
    pub fn new() -> Lexer {
        Lexer { pos: 0, braces: Vec::new() }
    }

    pub fn at(pos: usize) -> Lexer {
        Lexer { pos, braces: Vec::new() }
    }

    /// Skips whitespace and comments, recording comments. Returns whether a newline was seen.
    pub fn skip_trivia(&mut self, src: &str, comments: &mut BTreeMap<u32, Comment>) -> bool {
        let bytes = src.as_bytes();
        let mut nl = false;
        loop {
            let Some(&b) = bytes.get(self.pos) else { return nl };
            match b {
                b'\n' => {
                    nl = true;
                    self.pos += 1;
                }
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'/' if bytes.get(self.pos + 1) == Some(&b'/') => {
                    let start = self.pos;
                    while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                    let own_line = line_is_blank_before(src, start);
                    comments.entry(start as u32).or_insert(Comment {
                        span: Span::new(start as u32, self.pos as u32),
                        block: false,
                        own_line,
                    });
                }
                b'/' if bytes.get(self.pos + 1) == Some(&b'*') => {
                    let start = self.pos;
                    self.pos += 2;
                    while self.pos < bytes.len() && !(bytes[self.pos] == b'*' && bytes.get(self.pos + 1) == Some(&b'/'))
                    {
                        if bytes[self.pos] == b'\n' {
                            nl = true;
                        }
                        self.pos += 1;
                    }
                    self.pos = (self.pos + 2).min(bytes.len());
                    let own_line = line_is_blank_before(src, start);
                    comments.entry(start as u32).or_insert(Comment {
                        span: Span::new(start as u32, self.pos as u32),
                        block: true,
                        own_line,
                    });
                }
                _ => return nl,
            }
        }
    }

    pub fn next(&mut self, src: &str, comments: &mut BTreeMap<u32, Comment>) -> Token {
        let nl_before = self.skip_trivia(src, comments);
        let start = self.pos;
        let tok = |kind, end: usize, error| Token { kind, span: Span::new(start as u32, end as u32), nl_before, error };
        let rest = &src[self.pos..];
        let Some(c) = rest.chars().next() else {
            return tok(TokenKind::Eof, start, None);
        };
        if is_ident_start(c) {
            let len = rest.char_indices().find(|&(_, c)| !is_ident_continue(c)).map_or(rest.len(), |(i, _)| i);
            self.pos += len;
            return tok(TokenKind::Ident, self.pos, None);
        }
        if c.is_ascii_digit() || (c == '.' && rest[1..].starts_with(|c: char| c.is_ascii_digit())) {
            let bytes = rest.as_bytes();
            let mut i = 0;
            if rest.starts_with("0x") || rest.starts_with("0X") || rest.starts_with("0b") || rest.starts_with("0o") {
                i = 2;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
            } else {
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                        i += 1;
                    }
                } else if c == '.' {
                    i = 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                    let mut j = i + 1;
                    if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                        j += 1;
                    }
                    if j < bytes.len() && bytes[j].is_ascii_digit() {
                        i = j;
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                if i < bytes.len() && bytes[i] == b'n' {
                    i += 1;
                }
            }
            self.pos += i;
            return tok(TokenKind::Number, self.pos, None);
        }
        if c == '"' || c == '\'' {
            let bytes = rest.as_bytes();
            let mut i = 1;
            let mut error = Some("unterminated string literal");
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 2,
                    b'\n' => break,
                    b if b == c as u8 => {
                        i += 1;
                        error = None;
                        break;
                    }
                    _ => i += 1,
                }
            }
            self.pos += i.min(bytes.len());
            return tok(if error.is_some() { TokenKind::Error } else { TokenKind::String }, self.pos, error);
        }
        if c == '`' {
            self.pos += 1;
            let (kind, error) = self.scan_template(src, TokenKind::TemplateFull, TokenKind::TemplateHead);
            return tok(kind, self.pos, error);
        }
        if c == '}' && self.braces.last() == Some(&true) {
            self.braces.pop();
            self.pos += 1;
            let (kind, error) = self.scan_template(src, TokenKind::TemplateTail, TokenKind::TemplateMiddle);
            return tok(kind, self.pos, error);
        }
        for p in PUNCTS {
            if rest.starts_with(p) {
                self.pos += p.len();
                match *p {
                    "{" => self.braces.push(false),
                    "}" => {
                        self.braces.pop();
                    }
                    _ => {}
                }
                return tok(TokenKind::Punct, self.pos, None);
            }
        }
        self.pos += c.len_utf8();
        tok(TokenKind::Error, self.pos, Some("unexpected character"))
    }

    /// Scans template text after a backtick or a closing substitution brace.
    fn scan_template(
        &mut self,
        src: &str,
        end_kind: TokenKind,
        open_kind: TokenKind,
    ) -> (TokenKind, Option<&'static str>) {
        let bytes = src.as_bytes();
        while self.pos < bytes.len() {
            match bytes[self.pos] {
                b'\\' => self.pos += 2,
                b'`' => {
                    self.pos += 1;
                    return (end_kind, None);
                }
                b'$' if bytes.get(self.pos + 1) == Some(&b'{') => {
                    self.pos += 2;
                    self.braces.push(true);
                    return (open_kind, None);
                }
                _ => self.pos += 1,
            }
        }
        self.pos = bytes.len();
        (TokenKind::Error, Some("unterminated template literal"))
    }
}

impl Default for Lexer {
    fn default() -> Self {
        Lexer::new()
    }
}

fn line_is_blank_before(src: &str, offset: usize) -> bool {
    src[..offset].rsplit('\n').next().is_none_or(|line| line.trim().is_empty())
}

/// Tokenizes a whole file (used by tooling such as semantic highlighting).
pub fn tokenize(src: &str) -> (Vec<Token>, Vec<Comment>) {
    let mut comments = BTreeMap::new();
    let mut lexer = Lexer::new();
    let mut out = Vec::new();
    loop {
        let t = lexer.next(src, &mut comments);
        out.push(t);
        if t.kind == TokenKind::Eof {
            break;
        }
    }
    (out, comments.into_values().collect())
}
