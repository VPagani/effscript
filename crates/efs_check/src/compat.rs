//! The library versions the generated code is written for.
//!
//! Codegen targets one major version of Effect at a time (see
//! `docs/compatibility.md`). `efs check` reads the versions installed in the
//! project and warns when one is outside the tested range, so an upgrade that
//! breaks the generated code is reported as such rather than as a confusing
//! type error.

use std::path::{Path, PathBuf};

use crate::{CheckDiagnostic, Severity};

/// A package the generated code depends on, and the versions it is tested with.
pub struct Supported {
    pub package: &'static str,
    /// Version prefixes that are supported, e.g. `"4."`.
    pub prefixes: &'static [&'static str],
    /// The range as written in the docs and in warnings.
    pub range: &'static str,
}

pub const SUPPORTED: &[Supported] = &[
    Supported { package: "effect", prefixes: &["4."], range: "4.x" },
    Supported { package: "@typeonce/effect-machine", prefixes: &["0.47."], range: "0.47.x" },
];

/// Finds `node_modules/<package>/package.json` from `root` upwards.
fn installed(root: &Path, package: &str) -> Option<(PathBuf, String)> {
    for dir in root.ancestors() {
        let manifest = dir.join("node_modules").join(package).join("package.json");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            return Some((manifest, json.get("version")?.as_str()?.to_string()));
        }
    }
    None
}

/// Warnings for installed packages outside the supported ranges. Packages that
/// are not installed are skipped: a project need not use all of them.
pub fn check_versions(root: &Path) -> Vec<CheckDiagnostic> {
    SUPPORTED
        .iter()
        .filter_map(|s| {
            let (manifest, version) = installed(root, s.package)?;
            if s.prefixes.iter().any(|p| version.starts_with(p)) {
                return None;
            }
            Some(CheckDiagnostic {
                file: Some(manifest),
                span: None,
                text: None,
                message: format!(
                    "EffScript {} generates code for {} {}, but {} is installed; the generated code may not type check",
                    env!("CARGO_PKG_VERSION"),
                    s.package,
                    s.range,
                    version
                ),
                code: "efs(unsupported-version)".into(),
                severity: Severity::Warning,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(version: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("efs-compat-{version}-{}", std::process::id()));
        let pkg = dir.join("node_modules/effect");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("package.json"), format!(r#"{{ "name": "effect", "version": "{version}" }}"#)).unwrap();
        dir
    }

    #[test]
    fn supported_versions_pass() {
        let dir = project("4.1.6");
        assert!(check_versions(&dir.join("src")).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn other_majors_warn() {
        let dir = project("5.0.0");
        let warnings = check_versions(&dir);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("effect 4.x, but 5.0.0 is installed"), "{}", warnings[0].message);
        std::fs::remove_dir_all(dir).ok();
    }
}
