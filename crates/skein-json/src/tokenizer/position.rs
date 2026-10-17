//! Byte positions on the line being read (json.md, 3.6).

/// Where the tokenizer's last answer is in the document (json.md, 3.6).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Position {
    /// The bytes delivered before it, from 0.
    pub offset: u32,
    /// From 1: one more than the line feeds before it.
    pub line: u32,
    /// From 1, in bytes: one more than the bytes since the last line feed.
    pub column: u32,
}
impl Position {
    /// Before the first answer, and after restart.
    pub(crate) const START: Position = Position { offset: 0, line: 1, column: 1 };
}

/// The line being read: its number and its first byte's offset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Lines {
    line: u32,
    start: u32,
}
impl Lines {
    pub(crate) const FIRST: Lines = Lines { line: 1, start: 0 };
    /// A whitespace line feed at this offset ends the current line.
    pub(crate) fn feed(&mut self, offset: u32) {
        self.line = self.line.checked_add(1).expect("length keeps line within u32");
        self.start = offset.checked_add(1).expect("length keeps the end within u32");
    }
    pub(crate) fn at(self, offset: u32) -> Position {
        let column = offset.checked_sub(self.start).expect("offset is on the current line");
        Position { offset, line: self.line, column: column.checked_add(1).expect("length bounds the column") }
    }
}
