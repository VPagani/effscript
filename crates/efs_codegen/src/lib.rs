//! Lowers an EffScript module to TypeScript that targets Effect 4, `@typeonce/effect-machine`
//! and `@effect/ui`, together with a span map that relates every piece of generated text
//! to the EffScript source it came from.

mod component;
mod lower;
mod machine;
mod schema;
pub mod sourcemap;
mod writer;

use efs_syntax::{Diagnostic, Span};

pub use writer::{Segment, SegmentKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// TypeScript, with every type kept.
    #[default]
    TypeScript,
    /// JavaScript: types are dropped, and imports only used as types are removed.
    JavaScript,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub target: Target,
    /// The extension that replaces `.efs` in import specifiers (`.js` by default). Use
    /// `.efs` to keep specifiers unchanged, or an empty string to drop the extension.
    pub import_extension: String,
}

impl Default for Options {
    fn default() -> Self {
        Options { target: Target::TypeScript, import_extension: ".js".into() }
    }
}

#[derive(Clone, Debug)]
pub struct Output {
    pub code: String,
    /// Segments ordered by generated position, never overlapping in generated text.
    pub segments: Vec<Segment>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses and compiles EffScript source text.
pub fn compile(src: &str, options: &Options) -> Output {
    let parsed = efs_syntax::parse(src);
    let mut out = compile_module(src, &parsed.module, options);
    let mut diagnostics = parsed.diagnostics;
    diagnostics.append(&mut out.diagnostics);
    out.diagnostics = diagnostics;
    out
}

pub fn compile_module(src: &str, module: &efs_syntax::ast::Module, options: &Options) -> Output {
    if options.target == Target::JavaScript {
        // A first pass finds the imported names the generated code uses as values, so that
        // imports used only as types can be removed.
        let mut first = lower::Gen::new(src, options);
        first.keep_imports = None;
        let used = first.used_values(module);
        let mut second = lower::Gen::new(src, options);
        second.keep_imports = Some(used);
        return second.module(module);
    }
    lower::Gen::new(src, options).module(module)
}

/// The position of generated text in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapped {
    pub span: Span,
    /// Whether positions inside the span map one-to-one.
    pub exact: bool,
}

impl Output {
    fn segment_at(&self, gen_pos: u32) -> Option<&Segment> {
        let i = self.segments.partition_point(|s| s.gen_start + s.gen_len <= gen_pos);
        self.segments.get(i).filter(|s| s.gen_start <= gen_pos)
    }

    /// Maps a generated position to the source.
    pub fn to_source(&self, gen_pos: u32) -> Option<Mapped> {
        let s = self.segment_at(gen_pos).or_else(|| {
            // A position at the very end of a segment.
            let i = self.segments.partition_point(|s| s.gen_start + s.gen_len < gen_pos);
            self.segments.get(i).filter(|s| s.gen_start + s.gen_len == gen_pos)
        })?;
        Some(match s.kind {
            SegmentKind::Verbatim => {
                let p = s.src_start + (gen_pos - s.gen_start);
                Mapped { span: Span::new(p, p), exact: true }
            }
            SegmentKind::Atom => Mapped { span: Span::new(s.src_start, s.src_start + s.src_len), exact: false },
        })
    }

    /// Maps a generated range to a source range.
    pub fn range_to_source(&self, start: u32, len: u32) -> Option<Span> {
        let a = self.to_source(start)?;
        let end = start + len;
        let b = if len == 0 { a } else { self.to_source(end.saturating_sub(1)).unwrap_or(a) };
        if a.exact && b.exact {
            let e = if len == 0 { a.span.start } else { b.span.start + 1 };
            return Some(Span::new(a.span.start, e.max(a.span.start)));
        }
        if !a.exact && !b.exact && a.span == b.span {
            return Some(a.span);
        }
        Some(a.span.to(b.span))
    }

    /// Maps a source position to a generated position, preferring exact mappings.
    pub fn to_generated(&self, src_pos: u32) -> Option<u32> {
        let mut best: Option<(&Segment, u32)> = None;
        for s in &self.segments {
            let end = s.src_start + s.src_len;
            if s.kind == SegmentKind::Verbatim && s.src_start <= src_pos && src_pos < end.max(s.src_start + 1) {
                return Some(s.gen_start + (src_pos - s.src_start));
            }
            if s.src_start <= src_pos && src_pos <= end && best.is_none_or(|(b, _)| s.src_len < b.src_len) {
                best = Some((s, s.gen_start));
            }
        }
        best.map(|(_, g)| g)
    }

    /// The span map in the TypeScript content-mapper wire format:
    /// `[[virtualStart, virtualLength, originalStart, originalLength, kind], ...]`.
    pub fn span_map_json(&self) -> String {
        let mut s = String::from("[");
        for (i, seg) in self.segments.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let kind = match seg.kind {
                SegmentKind::Verbatim => 0,
                SegmentKind::Atom => 1,
            };
            s.push_str(&format!("[{},{},{},{},{}]", seg.gen_start, seg.gen_len, seg.src_start, seg.src_len, kind));
        }
        s.push(']');
        s
    }
}
