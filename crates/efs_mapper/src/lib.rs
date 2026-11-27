//! A content mapper for the TypeScript 7.1 compiler and language server (`tsgo`, `tsrs`).
//!
//! With the EffScript package listed under `contentMappers` in `tsconfig.json`, the compiler
//! starts `efs mapper` and asks it to transform each `.efs` file into TypeScript. The
//! reply carries a span map, so TypeScript's own diagnostics, hovers, completions and
//! go-to-definition work directly in `.efs` files.
//!
//! The protocol is JSON-RPC 2.0 with `Content-Length` framing over stdio:
//! `initialize`, `openProject`, `transform` and `closeProject`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};

use serde_json::{Value, json};

pub const DIAGNOSTIC_SOURCE: &str = "efs";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Utf8,
    Utf16,
}

pub struct Server {
    encoding: Encoding,
    projects: HashMap<String, efs_codegen::Options>,
}

impl Default for Server {
    fn default() -> Self {
        Server { encoding: Encoding::Utf8, projects: HashMap::new() }
    }
}

fn read_message(r: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let Some(len) = len else { return Ok(None) };
    let mut buf = vec![0; len];
    r.read_exact(&mut buf)?;
    Ok(serde_json::from_slice(&buf).ok())
}

fn write_message(w: &mut impl Write, v: &Value) -> std::io::Result<()> {
    let body = v.to_string();
    write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    w.flush()
}

/// Converts a byte offset in `text` to the negotiated encoding.
fn convert(text: &str, byte: u32, enc: Encoding) -> u32 {
    match enc {
        Encoding::Utf8 => byte,
        Encoding::Utf16 => {
            let mut b = (byte as usize).min(text.len());
            while !text.is_char_boundary(b) {
                b -= 1;
            }
            text[..b].encode_utf16().count() as u32
        }
    }
}

impl Server {
    /// Handles one request. Returns `Err(message)` for a JSON-RPC error.
    pub fn handle(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "initialize" => {
                let offered: Vec<&str> = params["positionEncodings"]
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                self.encoding =
                    if offered.is_empty() || offered.contains(&"utf-8") { Encoding::Utf8 } else { Encoding::Utf16 };
                let enc = if self.encoding == Encoding::Utf8 { "utf-8" } else { "utf-16" };
                Ok(json!({ "positionEncoding": enc, "diagnosticSource": DIAGNOSTIC_SOURCE }))
            }
            "openProject" => {
                let handle = params["projectHandle"].as_str().unwrap_or_default().to_string();
                let mut opts = efs_codegen::Options::default();
                if let Some(ext) = params["options"]["importExtension"].as_str() {
                    opts.import_extension = ext.to_string();
                }
                self.projects.insert(handle, opts);
                // The options come from tsconfig and are already part of the transform identity,
                // so no `configIdentity` is returned (that requires `dynamicConfig` in the manifest).
                Ok(json!({}))
            }
            "closeProject" => {
                if let Some(h) = params["projectHandle"].as_str() {
                    self.projects.remove(h);
                }
                Ok(Value::Null)
            }
            "transform" => {
                let content = params["content"].as_str().unwrap_or_default();
                let handle = params["projectHandle"].as_str().unwrap_or_default();
                let opts = self.projects.get(handle).cloned().unwrap_or_default();
                Ok(self.transform(content, &opts))
            }
            other => Err(format!("unknown method `{other}`")),
        }
    }

    pub fn transform(&self, content: &str, opts: &efs_codegen::Options) -> Value {
        let out = efs_codegen::compile(content, opts);
        let enc = self.encoding;
        let mappings: Vec<Value> = out
            .segments
            .iter()
            .map(|s| {
                let vs = convert(&out.code, s.gen_start, enc);
                let ve = convert(&out.code, s.gen_start + s.gen_len, enc);
                let os = convert(content, s.src_start, enc);
                let oe = convert(content, s.src_start + s.src_len, enc);
                let kind = match s.kind {
                    efs_codegen::SegmentKind::Verbatim => 0,
                    efs_codegen::SegmentKind::Atom => 1,
                };
                json!([vs, ve - vs, os, oe - os, kind])
            })
            .collect();
        let diagnostics: Vec<Value> = out
            .diagnostics
            .iter()
            .map(|d| {
                let s = convert(content, d.span.start, enc);
                let e = convert(content, d.span.end, enc);
                let code = if d.code == "syntax" { 1001 } else { 1002 };
                json!({ "messageText": d.message, "start": s, "length": e - s, "code": code })
            })
            .collect();
        let mut result = json!({ "text": out.code, "extension": ".ts", "mappings": mappings });
        if !diagnostics.is_empty() {
            result["diagnostics"] = Value::Array(diagnostics);
        }
        result
    }

    /// Serves requests on stdin/stdout until the client disconnects.
    pub fn run_stdio(&mut self) -> std::io::Result<()> {
        let stdin = std::io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        while let Some(msg) = read_message(&mut reader)? {
            let method = msg["method"].as_str().unwrap_or_default().to_string();
            if method == "exit" {
                break;
            }
            let id = msg.get("id").cloned();
            let result = self.handle(&method, &msg["params"]);
            // Notifications get no reply.
            let Some(id) = id else { continue };
            let reply = match result {
                Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
                Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": e } }),
            };
            write_message(&mut out, &reply)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_produces_valid_verbatim_mappings() {
        let src = "import { Effect } from \"effect\"\nexport effect function hello(name: string) {\n  run Effect.log(`héllo ${name}`)\n}\n";
        for enc in [Encoding::Utf8, Encoding::Utf16] {
            let s = Server { encoding: enc, projects: HashMap::new() };
            let r = s.transform(src, &Default::default());
            let text = r["text"].as_str().unwrap();
            let units = |t: &str| -> Vec<u16> { t.encode_utf16().collect() };
            let mut prev_end = 0;
            for m in r["mappings"].as_array().unwrap() {
                let m: Vec<u64> = m.as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect();
                assert!(m[0] >= prev_end, "segments overlap");
                prev_end = m[0] + m[1];
                if m[4] == 0 {
                    let (vs, vl, os, ol) = (m[0] as usize, m[1] as usize, m[2] as usize, m[3] as usize);
                    assert_eq!(vl, ol);
                    match enc {
                        Encoding::Utf8 => assert_eq!(&text.as_bytes()[vs..vs + vl], &src.as_bytes()[os..os + ol]),
                        Encoding::Utf16 => assert_eq!(units(text)[vs..vs + vl], units(src)[os..os + ol]),
                    }
                }
            }
        }
    }

    #[test]
    fn protocol_round_trip() {
        let mut s = Server::default();
        let init = s.handle("initialize", &json!({ "positionEncodings": ["utf-8", "utf-16"] })).unwrap();
        assert_eq!(init["positionEncoding"], "utf-8");
        s.handle("openProject", &json!({ "projectHandle": "p1", "configFileName": "/x/tsconfig.json" })).unwrap();
        let t = s
            .handle("transform", &json!({ "projectHandle": "p1", "fileName": "/x/a.efs", "content": "const x = 1" }))
            .unwrap();
        assert_eq!(t["text"], "const x = 1\n");
        assert_eq!(t["extension"], ".ts");
    }
}
