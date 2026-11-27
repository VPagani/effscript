//! Syntax for EffScript, a language that gives Effect a readable surface syntax.

pub mod ast;
pub mod diag;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod visit;

pub use diag::{Diagnostic, Severity};
pub use parser::{ParseResult, parse, parse_expr};
pub use span::{LineIndex, Span};
