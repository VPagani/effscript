//! Effect Language Service diagnostics, from `effect-tsgo` (the `@effect/tsgo` package).
//!
//! `efs check --effect` writes the build output next to each `.efs` file (as `efs build`
//! does), runs the project's `effect-tsgo` over it with the `@effect/language-service`
//! plugin, and maps each Effect diagnostic back to EffScript source through the span map.
//! Type errors are not taken from effect-tsgo: the in-process check reports those.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use efs_syntax::{LineIndex, Span};

use crate::{CheckDiagnostic, Session, Severity, generated_header, is_handwritten, output_path};

pub const PLUGIN: &str = "@effect/language-service";

/// Finds `effect-tsgo`: `$EFS_EFFECT_TSGO`, or `node_modules/.bin/effect-tsgo` at or above `root`.
pub fn find(root: &Path) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("EFS_EFFECT_TSGO") {
        return Some(PathBuf::from(p));
    }
    let exe = if cfg!(windows) { "effect-tsgo.cmd" } else { "effect-tsgo" };
    root.ancestors().map(|d| d.join("node_modules/.bin").join(exe)).find(|p| p.is_file())
}

/// One line of `tsc`-style output: `file(line,col): severity TS1234: message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawDiagnostic {
    pub file: String,
    /// One-based.
    pub line: u32,
    /// One-based, in UTF-16 code units.
    pub column: u32,
    pub severity: Severity,
    pub message: String,
    /// The Effect rule, from the `effect(rule)` suffix; `None` for ordinary TypeScript diagnostics.
    pub rule: Option<String>,
}

pub fn parse_output(out: &str) -> Vec<RawDiagnostic> {
    let mut diags: Vec<RawDiagnostic> = Vec::new();
    for line in out.lines() {
        if let Some(d) = parse_line(line) {
            diags.push(d);
        } else if line.starts_with(' ')
            && let Some(last) = diags.last_mut()
        {
            last.message.push('\n');
            last.message.push_str(line.trim());
        }
    }
    for d in &mut diags {
        if let Some(rest) = d.message.trim_end().strip_suffix(')')
            && let Some(i) = rest.rfind(" effect(")
        {
            d.rule = Some(rest[i + 8..].to_string());
            d.message = rest[..i].trim_end().to_string();
        }
    }
    diags
}

fn parse_line(line: &str) -> Option<RawDiagnostic> {
    let open = line.find('(')?;
    let close = open + line[open..].find("): ")?;
    let (l, c) = line[open + 1..close].split_once(',')?;
    let rest = &line[close + 3..];
    let (severity, rest) = rest.split_once(' ')?;
    let severity = match severity {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        "suggestion" | "message" => Severity::Suggestion,
        _ => return None,
    };
    let (_code, message) = rest.split_once(": ")?;
    Some(RawDiagnostic {
        file: line[..open].to_string(),
        line: l.trim().parse().ok()?,
        column: c.trim().parse().ok()?,
        severity,
        message: message.to_string(),
        rule: None,
    })
}

fn word_end(src: &str, start: usize) -> usize {
    src[start..]
        .char_indices()
        .find(|&(_, c)| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .map_or(src.len(), |(i, _)| start + i)
}

impl Session {
    /// Runs effect-tsgo and returns its Effect diagnostics, mapped to EffScript where they
    /// fall in generated code.
    pub fn effect_diagnostics(&mut self, exe: &Path) -> Result<Vec<CheckDiagnostic>, String> {
        let root = self.workspace.root.clone();
        let tsconfig = self.workspace.tsconfig.clone().ok_or("`--effect` needs a tsconfig.json")?;

        // 1. The build output, as `efs build` writes it.
        let files: Vec<_> = self.compile().values().cloned().collect();
        for c in &files {
            let ts = output_path(&c.path);
            if is_handwritten(&ts) || c.output.diagnostics.iter().any(|d| d.severity == efs_syntax::Severity::Error) {
                continue;
            }
            let name = c.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let text = generated_header(&name) + &c.output.code;
            if std::fs::read_to_string(&ts).ok().as_deref() != Some(text.as_str()) {
                std::fs::write(&ts, text).map_err(|e| format!("{}: {e}", ts.display()))?;
            }
        }

        // 2. Enable the Effect plugin if the project's tsconfig does not.
        let config_text = std::fs::read_to_string(&tsconfig).unwrap_or_default();
        let temp_config = (!config_text.contains(PLUGIN)).then(|| {
            let p = tsconfig.with_file_name(".efs-effect.tsconfig.json");
            let base = tsconfig.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let text = format!(
                "{{ \"extends\": \"./{base}\", \"compilerOptions\": {{ \"plugins\": [{{ \"name\": \"{PLUGIN}\" }}] }} }}\n"
            );
            std::fs::write(&p, text).map(|_| p)
        });
        let config = match &temp_config {
            Some(Ok(p)) => p.clone(),
            Some(Err(e)) => return Err(format!("cannot write a temporary tsconfig: {e}")),
            None => tsconfig.clone(),
        };

        // 3. Run it.
        let output = std::process::Command::new(exe)
            .args(["tsrs", "--noEmit", "--pretty", "false", "-p"])
            .arg(&config)
            .current_dir(&root)
            .output();
        if let Some(Ok(p)) = &temp_config {
            let _ = std::fs::remove_file(p);
        }
        let output = output.map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() && stdout.trim().is_empty() {
            return Err(format!("{} failed: {}", exe.display(), String::from_utf8_lossy(&output.stderr).trim()));
        }

        // 4. Keep the Effect diagnostics and map them back.
        let mut out = Vec::new();
        for d in parse_output(&stdout).into_iter().filter(|d| d.rule.is_some()) {
            let ts_path = std::fs::canonicalize(root.join(&d.file)).unwrap_or_else(|_| root.join(&d.file));
            let rule = d.rule.clone().unwrap_or_default();
            let code = format!("effect/{rule}");
            let source = files.iter().find(|c| output_path(&c.path) == ts_path);
            match source {
                Some(c) => {
                    // The written file starts with the one-line header.
                    let index = LineIndex::new(&c.output.code);
                    let generated = index.offset(d.line.saturating_sub(2), d.column.saturating_sub(1));
                    let Some(m) = c.output.to_source(generated) else { continue };
                    let span = if m.exact {
                        let s = m.span.start as usize;
                        Span::new(m.span.start, word_end(&c.src, s).max(s + 1).min(c.src.len()) as u32)
                    } else {
                        m.span
                    };
                    out.push(CheckDiagnostic {
                        file: Some(c.path.clone()),
                        span: Some(span),
                        text: Some(Arc::from(c.src.as_str())),
                        message: d.message,
                        code,
                        severity: d.severity,
                    });
                }
                None => {
                    let text = std::fs::read_to_string(&ts_path).unwrap_or_default();
                    let start = LineIndex::new(&text).offset(d.line.saturating_sub(1), d.column.saturating_sub(1));
                    out.push(CheckDiagnostic {
                        file: Some(ts_path),
                        span: Some(Span::new(start, start)),
                        text: Some(Arc::from(text.as_str())),
                        message: d.message,
                        code,
                        severity: d.severity,
                    });
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_effect_diagnostics() {
        let out = "src/a.ts(13,42): warning TS377033: This expression chains multiple `Effect.provide` calls. effect(multipleEffectProvide)\n\
                   src/b.ts(2,5): error TS2322: Type 'string' is not assignable to type 'number'.\n  more detail\n";
        let d = parse_output(out);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].rule.as_deref(), Some("multipleEffectProvide"));
        assert_eq!(d[0].message, "This expression chains multiple `Effect.provide` calls.");
        assert_eq!((d[0].line, d[0].column, d[0].severity), (13, 42, Severity::Warning));
        assert_eq!(d[1].rule, None);
        assert!(d[1].message.ends_with("more detail"));
    }
}
