use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    /// A stable identifier such as `syntax`, `codegen`, `lint/no-discarded-effect` or `ts2322`.
    pub code: String,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Error, span, message: message.into(), code: "syntax".into() }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Diagnostic {
        self.code = code.into();
        self
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Warning, span, message: message.into(), code: "syntax".into() }
    }
}
