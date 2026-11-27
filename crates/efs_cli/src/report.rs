//! Human-readable diagnostics.

use std::io::IsTerminal;
use std::path::Path;

use efs_syntax::{Diagnostic, LineIndex, Severity};

pub struct Reporter {
    color: bool,
    errors: usize,
    warnings: usize,
}

impl Reporter {
    pub fn new() -> Reporter {
        let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        Reporter { color, errors: 0, warnings: 0 }
    }

    fn paint(&self, code: &str, s: &str) -> String {
        if self.color { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
    }

    pub fn reset(&mut self) {
        self.errors = 0;
        self.warnings = 0;
    }

    #[allow(clippy::too_many_arguments)]
    fn print(
        &mut self,
        file: Option<&Path>,
        text: Option<&str>,
        start: Option<u32>,
        end: Option<u32>,
        severity: Severity,
        message: &str,
        code: &str,
    ) {
        let label = match severity {
            Severity::Error => {
                self.errors += 1;
                self.paint("1;31", "error")
            }
            Severity::Warning => {
                self.warnings += 1;
                self.paint("1;33", "warning")
            }
            Severity::Info => self.paint("1;36", "hint"),
        };
        let mut location = file.map(|f| f.display().to_string()).unwrap_or_default();
        let mut excerpt = String::new();
        if let (Some(text), Some(start)) = (text, start) {
            let index = LineIndex::new(text);
            let (line, col) = index.position(start);
            location = format!("{location}:{}:{}", line + 1, col + 1);
            if let Some(src_line) = text.lines().nth(line as usize) {
                let end = end.unwrap_or(start).max(start);
                let (eline, ecol) = index.position(end);
                let width = if eline == line {
                    (ecol.saturating_sub(col)).max(1)
                } else {
                    (src_line.chars().count() as u32).saturating_sub(col).max(1)
                };
                let gutter = format!("{}", line + 1);
                excerpt = format!(
                    "\n {} | {}\n {} | {}{}",
                    gutter,
                    src_line,
                    " ".repeat(gutter.len()),
                    " ".repeat(col as usize),
                    self.paint("1;31", &"^".repeat(width as usize))
                );
            }
        }
        let code = self.paint("2", &format!("[{code}]"));
        if location.is_empty() {
            eprintln!("{label}: {message} {code}{excerpt}");
        } else {
            eprintln!("{}: {label}: {message} {code}{excerpt}", self.paint("1", &location));
        }
    }

    pub fn diagnostic(&mut self, file: &Path, src: &str, d: &Diagnostic) {
        self.print(Some(file), Some(src), Some(d.span.start), Some(d.span.end), d.severity, &d.message, &d.code);
    }

    pub fn check_diagnostic(&mut self, d: &efs_check::CheckDiagnostic) {
        let severity = match d.severity {
            efs_check::Severity::Error => Severity::Error,
            efs_check::Severity::Warning => Severity::Warning,
            efs_check::Severity::Suggestion => Severity::Info,
        };
        let cwd = std::env::current_dir().unwrap_or_default();
        let file = d.file.as_ref().map(|f| f.strip_prefix(&cwd).unwrap_or(f).to_path_buf());
        self.print(
            file.as_deref(),
            d.text.as_deref(),
            d.span.map(|s| s.start),
            d.span.map(|s| s.end),
            severity,
            &d.message,
            &d.code,
        );
    }

    pub fn json(&mut self, d: &efs_check::CheckDiagnostic) {
        if d.severity == efs_check::Severity::Error {
            self.errors += 1;
        }
        let (line, col) = d.line_col().unwrap_or((0, 0));
        let v = serde_json::json!({
            "file": d.file.as_ref().map(|f| f.display().to_string()),
            "line": line + 1,
            "column": col + 1,
            "start": d.span.map(|s| s.start),
            "end": d.span.map(|s| s.end),
            "severity": format!("{:?}", d.severity).to_lowercase(),
            "code": d.code,
            "message": d.message,
        });
        println!("{v}");
    }

    pub fn error_line(&mut self, message: &str) {
        self.errors += 1;
        eprintln!("{}: {message}", self.paint("1;31", "error"));
    }

    pub fn summary(&self, what: &str) {
        let mut parts = vec![what.to_string()];
        if self.errors > 0 {
            parts.push(format!("{} error{}", self.errors, if self.errors == 1 { "" } else { "s" }));
        }
        if self.warnings > 0 {
            parts.push(format!("{} warning{}", self.warnings, if self.warnings == 1 { "" } else { "s" }));
        }
        eprintln!("{}", self.paint("2", &parts.join(", ")));
    }

    pub fn exit_code(&self, deny_warnings: bool) -> std::process::ExitCode {
        if self.errors > 0 || (deny_warnings && self.warnings > 0) {
            std::process::ExitCode::FAILURE
        } else {
            std::process::ExitCode::SUCCESS
        }
    }
}
