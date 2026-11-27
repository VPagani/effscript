//! `efs.json`: optional project configuration, found next to or above the given path.
//!
//! ```json
//! { "importExtension": ".js", "prelude": true, "outDir": "gen", "sourceMap": true, "emit": "js", "declaration": true, "lint": { "disabled": ["unused-binding"] }, "fmt": { "semicolons": false } }
//! ```

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub import_extension: String,
    /// Import the core Effect modules a file uses without importing them (default true).
    pub prelude: bool,
    pub out_dir: Option<PathBuf>,
    pub source_map: bool,
    /// Write JavaScript instead of TypeScript.
    pub emit_js: bool,
    /// With JavaScript output, also write `.d.ts` files.
    pub declaration: bool,
    pub lint_disabled: Vec<String>,
    /// `efs fmt` prints `;` terminators (default true).
    pub fmt_semicolons: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            import_extension: ".js".into(),
            prelude: true,
            out_dir: None,
            source_map: false,
            emit_js: false,
            declaration: false,
            lint_disabled: Vec::new(),
            fmt_semicolons: true,
        }
    }
}

impl Config {
    pub fn load(start: &Path) -> Config {
        let start = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
        let mut dir = if start.is_file() { start.parent().map(Path::to_path_buf) } else { Some(start) };
        while let Some(d) = dir {
            let candidate = d.join("efs.json");
            if let Ok(text) = std::fs::read_to_string(&candidate) {
                return Self::parse(&text, &d);
            }
            dir = d.parent().map(Path::to_path_buf);
        }
        Config::default()
    }

    fn parse(text: &str, base: &Path) -> Config {
        let mut c = Config::default();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
            eprintln!("warning: ignoring efs.json: invalid JSON");
            return c;
        };
        if let Some(e) = v["importExtension"].as_str() {
            c.import_extension = e.to_string();
        }
        if let Some(p) = v["prelude"].as_bool() {
            c.prelude = p;
        }
        if let Some(o) = v["outDir"].as_str() {
            c.out_dir = Some(base.join(o));
        }
        if let Some(s) = v["sourceMap"].as_bool() {
            c.source_map = s;
        }
        if let Some(e) = v["emit"].as_str() {
            c.emit_js = e == "js";
        }
        if let Some(d) = v["declaration"].as_bool() {
            c.declaration = d;
        }
        if let Some(rules) = v["lint"]["disabled"].as_array() {
            c.lint_disabled = rules.iter().filter_map(|r| r.as_str().map(String::from)).collect();
        }
        c.fmt_semicolons = efs_fmt::FormatOptions::from_config(&v).semicolons;
        c
    }

    pub fn fmt_options(&self) -> efs_fmt::FormatOptions {
        efs_fmt::FormatOptions { semicolons: self.fmt_semicolons }
    }

    pub fn codegen(&self) -> efs_codegen::Options {
        efs_codegen::Options {
            import_extension: self.import_extension.clone(),
            prelude: self.prelude,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn prelude_is_on_unless_turned_off() {
        assert!(Config::default().codegen().prelude);
        let c = Config::parse(r#"{ "prelude": false }"#, std::path::Path::new("."));
        assert!(!c.codegen().prelude);
    }
}
