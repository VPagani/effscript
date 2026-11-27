/// A byte range into a source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const DUMMY: Span = Span { start: 0, end: 0 };

    pub fn new(start: u32, end: u32) -> Span {
        Span { start, end }
    }

    pub fn to(self, other: Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end) }
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub fn contains(self, offset: u32) -> bool {
        self.start <= offset && offset <= self.end
    }

    pub fn text(self, src: &str) -> &str {
        &src[self.start as usize..self.end as usize]
    }
}

/// Converts byte offsets to zero-based (line, UTF-16 column) positions and back.
#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<u32>,
    text: String,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        LineIndex { line_starts, text: text.to_string() }
    }

    /// Zero-based line and UTF-16 column.
    pub fn position(&self, offset: u32) -> (u32, u32) {
        let offset = offset.min(self.text.len() as u32);
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let start = self.line_starts[line] as usize;
        let mut end = offset as usize;
        while !self.text.is_char_boundary(end) {
            end -= 1;
        }
        let col = self.text[start..end].encode_utf16().count() as u32;
        (line as u32, col)
    }

    /// Byte offset of a zero-based line and UTF-16 column.
    pub fn offset(&self, line: u32, col: u32) -> u32 {
        let Some(&start) = self.line_starts.get(line as usize) else {
            return self.text.len() as u32;
        };
        let end = self.line_starts.get(line as usize + 1).copied().unwrap_or(self.text.len() as u32);
        let mut units = 0;
        for (i, ch) in self.text[start as usize..end as usize].char_indices() {
            if units >= col {
                return start + i as u32;
            }
            units += ch.len_utf16() as u32;
        }
        end
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}
