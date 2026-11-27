//! Source Map v3 generation from the span map.

use efs_syntax::LineIndex;

use crate::{Output, SegmentKind};

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn vlq(out: &mut String, value: i64) {
    let mut v = if value < 0 { ((-value) << 1) | 1 } else { value << 1 };
    loop {
        let mut digit = (v & 31) as usize;
        v >>= 5;
        if v > 0 {
            digit |= 32;
        }
        out.push(B64[digit] as char);
        if v == 0 {
            break;
        }
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Builds a Source Map v3 JSON document for `output`, generated from `source_name`.
pub fn source_map(output: &Output, src: &str, source_name: &str, file: &str) -> String {
    let gen_index = LineIndex::new(&output.code);
    let src_index = LineIndex::new(src);
    let mut lines: Vec<Vec<(u32, u32, u32)>> = vec![Vec::new(); gen_index.line_count()];
    for seg in &output.segments {
        let (gl, gc) = gen_index.position(seg.gen_start);
        let (sl, sc) = src_index.position(seg.src_start);
        lines[gl as usize].push((gc, sl, sc));
        // Verbatim segments that span lines need a mapping at each line start.
        if seg.kind == SegmentKind::Verbatim {
            let text = &output.code[seg.gen_start as usize..(seg.gen_start + seg.gen_len) as usize];
            for (i, _) in text.match_indices('\n') {
                let g = seg.gen_start + i as u32 + 1;
                let (gl, gc) = gen_index.position(g);
                let (sl, sc) = src_index.position(seg.src_start + i as u32 + 1);
                if (gl as usize) < lines.len() {
                    lines[gl as usize].push((gc, sl, sc));
                }
            }
        }
    }
    let mut mappings = String::new();
    let (mut prev_sl, mut prev_sc) = (0i64, 0i64);
    for (i, line) in lines.iter_mut().enumerate() {
        if i > 0 {
            mappings.push(';');
        }
        line.sort();
        line.dedup_by_key(|e| e.0);
        let mut prev_gc = 0i64;
        for (j, &(gc, sl, sc)) in line.iter().enumerate() {
            if j > 0 {
                mappings.push(',');
            }
            vlq(&mut mappings, gc as i64 - prev_gc);
            vlq(&mut mappings, 0);
            vlq(&mut mappings, sl as i64 - prev_sl);
            vlq(&mut mappings, sc as i64 - prev_sc);
            prev_gc = gc as i64;
            prev_sl = sl as i64;
            prev_sc = sc as i64;
        }
    }
    format!(
        "{{\"version\":3,\"file\":{},\"sources\":[{}],\"sourcesContent\":[{}],\"names\":[],\"mappings\":{}}}",
        json_str(file),
        json_str(source_name),
        json_str(src),
        json_str(&mappings)
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn vlq_encodes() {
        let mut s = String::new();
        super::vlq(&mut s, 0);
        super::vlq(&mut s, 1);
        super::vlq(&mut s, -1);
        super::vlq(&mut s, 16);
        assert_eq!(s, "ACDgB");
    }
}
