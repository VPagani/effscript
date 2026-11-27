//! The EffScript compiler as a WebAssembly module with a minimal C ABI, so that it runs in
//! Node.js, Bun and Deno without native binaries or generated bindings.
//!
//! The host writes UTF-8 input into memory from `efs_alloc`, calls `efs_compile`, and reads
//! the JSON result at the returned address (high 32 bits) and length (low 32 bits), then
//! releases both buffers with `efs_free`.

use serde_json::{Value, json};

/// Compiles `source` with JSON `options`
/// (`{ filename, target: "js" | "ts", importExtension, sourceMap }`) and returns a JSON result
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
    let out = efs_codegen::compile(source, &o);
    let index = efs_syntax::LineIndex::new(source);
    let diagnostics: Vec<Value> = out
        .diagnostics
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
        .collect();
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

/// # Safety
/// The two buffers must be valid UTF-8 written into memory from `efs_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn efs_compile(src: *const u8, src_len: usize, opts: *const u8, opts_len: usize) -> u64 {
    let source = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(src, src_len)) };
    let options = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(opts, opts_len)) };
    let result = compile_json(source, options).into_bytes().into_boxed_slice();
    let len = result.len();
    let ptr = Box::into_raw(result) as *mut u8;
    ((ptr as u64) << 32) | len as u64
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
}
