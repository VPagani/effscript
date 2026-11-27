//! The EffScript compiler as a WebAssembly module with a minimal C ABI, so that it runs in
//! Node.js, Bun and Deno without native binaries or generated bindings.
//!
//! The host writes UTF-8 input into memory from `efs_alloc`, calls `efs_compile`,
//! `efs_format` or `efs_lint` with the source and JSON options, and reads the JSON result at
//! the returned address (high 32 bits) and length (low 32 bits), then releases the buffers
//! with `efs_free`.

use serde_json::{Value, json};

/// Compiles `source` with JSON `options`
/// (`{ filename, target: "js" | "ts", importExtension, prelude, sourceMap }`) and returns a JSON result
/// (`{ code, map, diagnostics: [{ message, code, severity, start, end, line, column }] }`).
pub fn compile_json(source: &str, options: &str) -> String {
    let opts: Value = serde_json::from_str(options).unwrap_or(Value::Null);
    let filename = opts["filename"].as_str().unwrap_or("input.efs");
    let mut o = efs_codegen::Options::default();
    if opts["target"].as_str() == Some("js") {
        o.target = efs_codegen::Target::JavaScript;
    }
    if let Some(ext) = opts["importExtension"].as_str() {
        o.import_extension = ext.to_string();
    }
    if let Some(p) = opts["prelude"].as_bool() {
        o.prelude = p;
    }
    let out = efs_codegen::compile(source, &o);
    let diagnostics = diagnostics_json(source, &out.diagnostics);
    let map = if opts["sourceMap"].as_bool().unwrap_or(true) {
        let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
        let ext = if o.target == efs_codegen::Target::JavaScript { "js" } else { "ts" };
        let file = format!("{}.{ext}", base.strip_suffix(".efs").unwrap_or(base));
        Some(efs_codegen::sourcemap::source_map(&out, source, base, &file))
    } else {
        None
    };
    json!({ "code": out.code, "map": map, "diagnostics": diagnostics }).to_string()
}

fn diagnostics_json(source: &str, diagnostics: &[efs_syntax::Diagnostic]) -> Vec<Value> {
    let index = efs_syntax::LineIndex::new(source);
    diagnostics
        .iter()
        .map(|d| {
            let (line, column) = index.position(d.span.start);
            json!({
                "message": d.message,
                "code": d.code,
                "severity": match d.severity {
                    efs_syntax::Severity::Error => "error",
                    efs_syntax::Severity::Warning => "warning",
                    efs_syntax::Severity::Info => "info",
                },
                "start": d.span.start,
                "end": d.span.end,
                "line": line + 1,
                "column": column + 1,
            })
        })
        .collect()
}

/// Formats `source` with JSON `options` (`{ semicolons }`) and returns `{ code }`, or
/// `{ diagnostics }` when the source does not parse.
pub fn format_json(source: &str, options: &str) -> String {
    let opts: Value = serde_json::from_str(options).unwrap_or(Value::Null);
    let mut o = efs_fmt::FormatOptions::default();
    if let Some(s) = opts["semicolons"].as_bool() {
        o.semicolons = s;
    }
    match efs_fmt::format_with(source, &o) {
        Ok(code) => json!({ "code": code, "diagnostics": [] }).to_string(),
        Err(d) => json!({ "code": null, "diagnostics": diagnostics_json(source, &d) }).to_string(),
    }
}

/// Lints `source` with JSON `options` (`{ disabled: [rule] }`) and returns `{ diagnostics }`.
pub fn lint_json(source: &str, options: &str) -> String {
    let opts: Value = serde_json::from_str(options).unwrap_or(Value::Null);
    let disabled = opts["disabled"]
        .as_array()
        .map(|a| a.iter().filter_map(|r| r.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let d = efs_lint::lint_source(source, &efs_lint::Config { disabled });
    json!({ "diagnostics": diagnostics_json(source, &d) }).to_string()
}

#[unsafe(no_mangle)]
pub extern "C" fn efs_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` and `len` must come from `efs_alloc(len)` or a result of `efs_compile`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn efs_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(unsafe { Vec::from_raw_parts(ptr, 0, len.max(1)) });
    }
}

/// Runs `f` on the source and options buffers and returns its JSON result as a packed
/// address and length.
///
/// # Safety
/// The two buffers must be valid UTF-8 written into memory from `efs_alloc`.
unsafe fn call(src: *const u8, src_len: usize, opts: *const u8, opts_len: usize, f: fn(&str, &str) -> String) -> u64 {
    let source = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(src, src_len)) };
    let options = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(opts, opts_len)) };
    let result = f(source, options).into_bytes().into_boxed_slice();
    let len = result.len();
    let ptr = Box::into_raw(result) as *mut u8;
    ((ptr as u64) << 32) | len as u64
}

/// # Safety
/// The two buffers must be valid UTF-8 written into memory from `efs_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn efs_compile(src: *const u8, src_len: usize, opts: *const u8, opts_len: usize) -> u64 {
    unsafe { call(src, src_len, opts, opts_len, compile_json) }
}

/// # Safety
/// The two buffers must be valid UTF-8 written into memory from `efs_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn efs_format(src: *const u8, src_len: usize, opts: *const u8, opts_len: usize) -> u64 {
    unsafe { call(src, src_len, opts, opts_len, format_json) }
}

/// # Safety
/// The two buffers must be valid UTF-8 written into memory from `efs_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn efs_lint(src: *const u8, src_len: usize, opts: *const u8, opts_len: usize) -> u64 {
    unsafe { call(src, src_len, opts, opts_len, lint_json) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiles_to_javascript_with_a_map() {
        let r: serde_json::Value = serde_json::from_str(&super::compile_json(
            "export const n: number = 1\n",
            r#"{ "filename": "src/a.efs", "target": "js" }"#,
        ))
        .unwrap();
        assert_eq!(r["code"], "export const n = 1\n");
        assert!(r["map"].as_str().unwrap().contains("\"sources\":[\"a.efs\"]"));
        assert_eq!(r["diagnostics"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn formats_and_reports_parse_errors() {
        let r: serde_json::Value = serde_json::from_str(&super::format_json("const  n = 1", "{}")).unwrap();
        assert_eq!(r["code"], "const n = 1;\n");
        let r: serde_json::Value =
            serde_json::from_str(&super::format_json("const n = 1", r#"{ "semicolons": false }"#)).unwrap();
        assert_eq!(r["code"], "const n = 1\n");
        let r: serde_json::Value = serde_json::from_str(&super::format_json("const = 1", "{}")).unwrap();
        assert!(r["code"].is_null());
        assert_eq!(r["diagnostics"][0]["line"], 1);
    }

    #[test]
    fn lints_with_disabled_rules() {
        let src = "effect function f() {\n  let n = 1;\n  return n;\n}\n";
        let r: serde_json::Value = serde_json::from_str(&super::lint_json(src, "{}")).unwrap();
        assert_eq!(r["diagnostics"][0]["code"], "lint/prefer-const");
        let r: serde_json::Value =
            serde_json::from_str(&super::lint_json(src, r#"{ "disabled": ["prefer-const"] }"#)).unwrap();
        assert_eq!(r["diagnostics"].as_array().unwrap().len(), 0);
    }
}
