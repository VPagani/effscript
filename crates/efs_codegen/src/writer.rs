use efs_syntax::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    /// Generated text equal to the source text; positions map one-to-one.
    Verbatim,
    /// Generated text derived from a source span as a whole.
    Atom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub gen_start: u32,
    pub gen_len: u32,
    pub src_start: u32,
    pub src_len: u32,
    pub kind: SegmentKind,
}

pub struct Writer<'s> {
    src: &'s str,
    pub out: String,
    pub segs: Vec<Segment>,
    nodes: Vec<Span>,
    indent: usize,
}

impl<'s> Writer<'s> {
    pub fn new(src: &'s str) -> Writer<'s> {
        Writer { src, out: String::new(), segs: Vec::new(), nodes: Vec::new(), indent: 0 }
    }

    pub fn pos(&self) -> u32 {
        self.out.len() as u32
    }

    pub fn push_node(&mut self, span: Span) {
        self.nodes.push(span);
    }

    pub fn pop_node(&mut self) {
        self.nodes.pop();
    }

    /// Writes synthesized text, attributed to the innermost node.
    pub fn s(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let start = self.pos();
        self.out.push_str(text);
        let Some(&node) = self.nodes.last() else { return };
        if let Some(last) = self.segs.last_mut()
            && last.kind == SegmentKind::Atom
            && last.src_start == node.start
            && last.src_len == node.len()
            && last.gen_start + last.gen_len == start
        {
            last.gen_len += text.len() as u32;
            return;
        }
        self.segs.push(Segment {
            gen_start: start,
            gen_len: text.len() as u32,
            src_start: node.start,
            src_len: node.len(),
            kind: SegmentKind::Atom,
        });
    }

    /// Writes synthesized text attributed to a specific span.
    pub fn s_at(&mut self, span: Span, text: &str) {
        self.push_node(span);
        self.s(text);
        self.pop_node();
    }

    /// Copies source text verbatim.
    pub fn v(&mut self, span: Span) {
        let text = span.text(self.src);
        if text.is_empty() {
            return;
        }
        let start = self.pos();
        self.out.push_str(text);
        self.segs.push(Segment {
            gen_start: start,
            gen_len: span.len(),
            src_start: span.start,
            src_len: span.len(),
            kind: SegmentKind::Verbatim,
        });
    }

    pub fn nl(&mut self) {
        self.s("\n");
        let pad = "  ".repeat(self.indent);
        self.s(&pad);
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }

    pub fn dedent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }
}
