//! Patch tokens and statement grammar (codec.md, 11.1).

use super::{Expected, Header, LANGUAGE, Op, PatchError, PatchWrong, Span, add, error, spelled};
use skein_lib::List;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Word,
    Text,
    Mark,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) at: Span,
    pub(crate) line: u32,
    pub(crate) kind: Kind,
}
impl Token {
    pub(crate) fn error(self, wrong: PatchWrong) -> PatchError {
        error(self.line, self.at, wrong)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Range {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
    Operation(Op),
    Rename,
    Site,
    Refusal,
}

/// Unresolved statement: names are spans, types and values token ranges.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Line {
    pub(crate) token: Token,
    pub(crate) kind: LineKind,
    pub(crate) declaration: Span,
    pub(crate) part: Option<Span>,
    pub(crate) other: Span,
    pub(crate) old_type: Range,
    pub(crate) new_type: Range,
    pub(crate) value: Range,
    pub(crate) index: u32,
    pub(crate) site: u32,
}
impl Line {
    pub(crate) fn error(self, wrong: PatchWrong) -> PatchError {
        error(self.token.line, self.declaration, wrong)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Block {
    pub(crate) token: Token,
    pub(crate) family: Span,
    pub(crate) from: u16,
    pub(crate) to: u16,
    pub(crate) lines: Range,
}
impl Block {
    pub(crate) fn error(self, wrong: PatchWrong) -> PatchError {
        self.token.error(wrong)
    }
}

#[derive(Debug)]
pub(crate) struct Parsed {
    pub(crate) header: Header,
    pub(crate) tokens: List<Token>,
    pub(crate) lines: List<Line>,
    pub(crate) blocks: List<Block>,
    pub(crate) marks: List<Span>,
}

#[derive(Debug)]
struct Cursor {
    offset: u32,
    line: u32,
    active: bool,
    ended: bool,
}
impl Cursor {
    fn new() -> Cursor {
        Cursor { offset: 0, line: 1, active: false, ended: false }
    }
    fn next(&mut self, text: &[u8]) -> Result<Option<Token>, PatchError> {
        if self.ended {
            return Ok(None);
        }
        let Ok(length) = u32::try_from(text.len()) else {
            return Err(error(1, Span { start: 0, end: 0 }, PatchWrong::Long));
        };
        let mut comment = false;
        for _ in self.offset..length {
            let byte = *text.get(usize::try_from(self.offset).expect("u32 fits usize")).expect("offset before end");
            let start = self.offset;
            self.offset = add(self.offset, 1);
            if byte == b'\n' {
                let line = self.line;
                self.line = add(self.line, 1);
                comment = false;
                if self.active {
                    self.active = false;
                    return Ok(Some(Token { at: Span { start, end: self.offset }, line, kind: Kind::End }));
                }
            } else if comment {
            } else if byte == b'#' {
                comment = true;
            } else if byte == b' ' || byte == b'\t' {
            } else {
                let kind = if word_byte(byte) {
                    for _ in self.offset..length {
                        let byte = *text
                            .get(usize::try_from(self.offset).expect("u32 fits usize"))
                            .expect("offset before end");
                        if !word_byte(byte) {
                            break;
                        }
                        self.offset = add(self.offset, 1);
                    }
                    Kind::Word
                } else if byte == b'"' {
                    let mut closed = false;
                    for _ in self.offset..length {
                        let byte = *text
                            .get(usize::try_from(self.offset).expect("u32 fits usize"))
                            .expect("offset before end");
                        let at = self.offset;
                        self.offset = add(self.offset, 1);
                        if byte == b'"' {
                            closed = true;
                            break;
                        }
                        if byte == b'\\' || byte < 0x20 || byte == 0x7f {
                            return Err(error(self.line, Span { start: at, end: self.offset }, PatchWrong::Character));
                        }
                    }
                    if !closed {
                        return Err(error(
                            self.line,
                            Span { start, end: self.offset },
                            PatchWrong::Expected(Expected::Text),
                        ));
                    }
                    let end = self.offset.checked_sub(1).expect("closing quote consumed");
                    if !crate::text_is_valid(spelled(text, Span { start: add(start, 1), end })) {
                        return Err(error(self.line, Span { start, end: self.offset }, PatchWrong::Value));
                    }
                    Kind::Text
                } else if byte == b'.' || byte == b'?' || byte == b'[' || byte == b']' {
                    Kind::Mark
                } else {
                    return Err(error(self.line, Span { start, end: self.offset }, PatchWrong::Character));
                };
                self.active = true;
                return Ok(Some(Token { at: Span { start, end: self.offset }, line: self.line, kind }));
            }
        }
        self.ended = true;
        if self.active {
            self.active = false;
            return Ok(Some(Token { at: Span { start: length, end: length }, line: self.line, kind: Kind::End }));
        }
        Ok(None)
    }
}

fn word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

// The cursor has indices only. Header mode reads tokens directly without a heap allocation.
#[derive(Debug)]
struct Reader {
    tokens: List<Token>,
    cursor: Cursor,
    streaming: bool,
    at: u32,
    look: Option<Token>,
}
impl Reader {
    fn peek(&mut self, text: &[u8]) -> Result<Option<Token>, PatchError> {
        if self.streaming {
            if self.look.is_none() {
                self.look = self.cursor.next(text)?;
            }
            Ok(self.look)
        } else {
            Ok(self.tokens.get(self.at).copied())
        }
    }
    fn is(&mut self, text: &[u8], word: &[u8]) -> Result<bool, PatchError> {
        match self.peek(text)? {
            Some(token) => Ok(token.kind != Kind::End && spelled(text, token.at) == word),
            None => Ok(false),
        }
    }
    fn take(&mut self, text: &[u8], wanted: Expected) -> Result<Token, PatchError> {
        let Some(token) = self.peek(text)? else {
            let end = u32::try_from(text.len()).expect("text length checked");
            let mut line = 1_u32;
            for &byte in text {
                if byte == b'\n' {
                    line = add(line, 1);
                }
            }
            if text.last() == Some(&b'\n') {
                line = line.checked_sub(1).expect("newline counted").max(1);
            }
            return Err(error(line, Span { start: end, end }, PatchWrong::Expected(wanted)));
        };
        self.at = add(self.at, 1);
        self.look = None;
        Ok(token)
    }
    fn expect(&mut self, text: &[u8], word: &[u8], wanted: Expected) -> Result<Token, PatchError> {
        let token = self.take(text, wanted)?;
        if token.kind == Kind::End || spelled(text, token.at) != word {
            return Err(token.error(PatchWrong::Expected(wanted)));
        }
        Ok(token)
    }
    fn word(&mut self, text: &[u8], wanted: Expected) -> Result<Token, PatchError> {
        let token = self.take(text, wanted)?;
        if token.kind != Kind::Word {
            return Err(token.error(PatchWrong::Expected(wanted)));
        }
        Ok(token)
    }
    fn name(&mut self, text: &[u8], wanted: Expected) -> Result<Token, PatchError> {
        let token = self.word(text, wanted)?;
        if !name(spelled(text, token.at)) {
            return Err(token.error(PatchWrong::Expected(wanted)));
        }
        Ok(token)
    }
    fn number(&mut self, text: &[u8], wanted: Expected, max: u32) -> Result<u32, PatchError> {
        let token = self.word(text, wanted)?;
        let bytes = spelled(text, token.at);
        for &byte in bytes {
            if !byte.is_ascii_digit() {
                return Err(token.error(PatchWrong::Expected(wanted)));
            }
        }
        let number = decimal(bytes).ok_or(token.error(PatchWrong::Number))?;
        if number > u64::from(max) {
            return Err(token.error(PatchWrong::Number));
        }
        Ok(u32::try_from(number).expect("number checked against u32"))
    }
    fn end(&mut self, text: &[u8]) -> Result<(), PatchError> {
        let token = self.take(text, Expected::End)?;
        if token.kind != Kind::End {
            return Err(token.error(PatchWrong::Expected(Expected::End)));
        }
        Ok(())
    }
    fn ty(&mut self, text: &[u8]) -> Result<Range, PatchError> {
        let start = self.at;
        for _ in 0..text.len() {
            let token = self.name(text, Expected::Type)?;
            match spelled(text, token.at) {
                b"option" => continue,
                b"list" => {
                    self.number(text, Expected::Type, u32::MAX)?;
                    continue;
                }
                b"fixed" | b"secret" | b"text" => {
                    self.number(text, Expected::Type, u32::MAX)?;
                }
                b"bytes" => {
                    self.number(text, Expected::Type, u32::MAX)?;
                    if self.is(text, b"holds")? {
                        self.take(text, Expected::Type)?;
                        self.word(text, Expected::Family)?;
                        self.name(text, Expected::Name)?;
                    } else if self.is(text, b"pieces")? {
                        self.take(text, Expected::Type)?;
                        self.name(text, Expected::Name)?;
                    }
                }
                _ => {}
            }
            return Ok(Range { start, end: self.at });
        }
        self.take(text, Expected::Type)?;
        unreachable!("a type consumes at least one token per turn")
    }
    fn value(&mut self, text: &[u8]) -> Result<Range, PatchError> {
        let start = self.at;
        for _ in 0..text.len() {
            let token = self.take(text, Expected::Value)?;
            match token.kind {
                Kind::Word => {
                    if spelled(text, token.at) == b"some" {
                        continue;
                    }
                }
                Kind::Text => {}
                Kind::Mark => {
                    if spelled(text, token.at) == b"[" {
                        self.expect(text, b"]", Expected::Value)?;
                    } else if spelled(text, token.at) != b"?" {
                        return Err(token.error(PatchWrong::Expected(Expected::Value)));
                    }
                }
                Kind::End => return Err(token.error(PatchWrong::Expected(Expected::Value))),
            }
            return Ok(Range { start, end: self.at });
        }
        self.take(text, Expected::Value)?;
        unreachable!("a value consumes at least one token per turn")
    }
}

pub(crate) fn decimal(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0_u64;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(byte.checked_sub(b'0')?))?;
    }
    Some(value)
}
fn name(bytes: &[u8]) -> bool {
    let mut bytes = bytes.iter();
    let Some(&first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != b'_' {
        return false;
    }
    for &byte in bytes {
        if !byte.is_ascii_alphanumeric() && byte != b'_' {
            return false;
        }
    }
    true
}

fn read_header(reader: &mut Reader, text: &[u8]) -> Result<Header, PatchError> {
    reader.expect(text, b"patch", Expected::Language)?;
    let token = reader.peek(text)?;
    let language = reader.number(text, Expected::Language, u32::from(u16::MAX))?;
    if language != u32::from(LANGUAGE) {
        return Err(token.expect("number consumed a token").error(PatchWrong::Language));
    }
    reader.end(text)?;
    reader.expect(text, b"owner", Expected::Owner)?;
    let owner = reader.word(text, Expected::Family)?.at;
    let level = reader.number(text, Expected::Level, u32::MAX)?;
    let mut after = Span { start: 0, end: 0 };
    if reader.is(text, b"after")? {
        reader.take(text, Expected::Family)?;
        after.start = reader.word(text, Expected::Family)?.at.start;
        reader.number(text, Expected::Level, u32::MAX)?;
        for _ in 0..text.len() {
            let Some(token) = reader.peek(text)? else {
                break;
            };
            if token.kind == Kind::End {
                after.end = token.at.start;
                break;
            }
            reader.word(text, Expected::Family)?;
            reader.number(text, Expected::Level, u32::MAX)?;
        }
    }
    reader.end(text)?;
    let mut marks = Span { start: 0, end: 0 };
    for _ in 0..text.len() {
        if !reader.is(text, b"mark")? {
            break;
        }
        let token = reader.take(text, Expected::Line)?;
        if marks.start == 0 {
            marks.start = token.at.start;
        }
        let mark = reader.word(text, Expected::Family)?;
        marks.end = mark.at.end;
        reader.end(text)?;
    }
    Ok(Header { owner, level, after, marks })
}

/// Header-only streaming parse, stopping before the first family statement.
pub(crate) fn header(text: &[u8]) -> Result<Header, PatchError> {
    if u32::try_from(text.len()).is_err() {
        return Err(error(1, Span { start: 0, end: 0 }, PatchWrong::Long));
    }
    let mut reader =
        Reader { tokens: List::with_capacity(0), cursor: Cursor::new(), streaming: true, at: 0, look: None };
    let header = read_header(&mut reader, text)?;
    if let Some(token) = reader.peek(text)?
        && spelled(text, token.at) != b"family"
    {
        return Err(token.error(PatchWrong::Expected(Expected::Line)));
    }
    Ok(header)
}

pub(crate) fn after(text: &[u8], range: Span, upstream: &[u8]) -> Option<u32> {
    let bytes = text.get(usize::try_from(range.start).ok()?..usize::try_from(range.end).ok()?)?;
    let mut cursor = Cursor::new();
    for _ in 0..bytes.len() {
        let token = cursor.next(bytes).ok()??;
        if token.kind == Kind::End {
            return None;
        }
        let level = cursor.next(bytes).ok()??;
        if spelled(bytes, token.at) == upstream {
            return u32::try_from(decimal(spelled(bytes, level.at))?).ok();
        }
    }
    None
}
pub(crate) fn marked(text: &[u8], range: Span, mark: &[u8]) -> bool {
    let Some(bytes) = text.get(
        usize::try_from(range.start).expect("u32 fits usize")..usize::try_from(range.end).expect("u32 fits usize"),
    ) else {
        return false;
    };
    let mut cursor = Cursor::new();
    for _ in 0..bytes.len() {
        let Ok(Some(token)) = cursor.next(bytes) else {
            return false;
        };
        if token.kind != Kind::End && spelled(bytes, token.at) == b"mark" {
            match cursor.next(bytes) {
                Ok(Some(token)) => {
                    if spelled(bytes, token.at) == mark {
                        return true;
                    }
                }
                Ok(None) | Err(_) => return false,
            }
        }
    }
    false
}

pub(crate) fn parse(text: &[u8]) -> Result<Parsed, PatchError> {
    let Ok(length) = u32::try_from(text.len()) else {
        return Err(error(1, Span { start: 0, end: 0 }, PatchWrong::Long));
    };
    let mut cursor = Cursor::new();
    let mut count = 0;
    for _ in 0..=length {
        if cursor.next(text)?.is_none() {
            break;
        }
        count = add(count, 1);
    }
    let mut reader =
        Reader { tokens: List::with_capacity(count), cursor: Cursor::new(), streaming: false, at: 0, look: None };
    for _ in 0..count {
        let token = reader.cursor.next(text)?.expect("first pass counted tokens");
        reader.tokens.push(token).expect("tokens counted before allocation");
    }
    let header = read_header(&mut reader, text)?;
    let mut parsed = Parsed {
        header,
        tokens: List::with_capacity(0),
        lines: List::with_capacity(count),
        blocks: List::with_capacity(count),
        marks: List::with_capacity(count),
    };
    let mut mark_name = false;
    for token in &reader.tokens {
        if token.at.start >= header.marks.start && token.at.end <= header.marks.end && token.kind == Kind::Word {
            if mark_name {
                parsed.marks.push(token.at).expect("marks bounded by token count");
            }
            mark_name = !mark_name;
        }
    }
    let mut sites = 0;
    let mut refusals = 0;
    for _ in 0..count {
        if reader.peek(text)?.is_none() {
            break;
        }
        let token = reader.expect(text, b"family", Expected::Line)?;
        let family = reader.word(text, Expected::Family)?.at;
        for block in &parsed.blocks {
            if spelled(text, block.family) == spelled(text, family) {
                return Err(token.error(PatchWrong::Twice));
            }
        }
        let from =
            u16::try_from(reader.number(text, Expected::Version, u32::from(u16::MAX))?).expect("version checked");
        reader.expect(text, b"to", Expected::To)?;
        let to = u16::try_from(reader.number(text, Expected::Version, u32::from(u16::MAX))?).expect("version checked");
        reader.end(text)?;
        let start = parsed.lines.len();
        let mut site = None;
        for _ in 0..count {
            if reader.peek(text)?.is_none() || reader.is(text, b"family")? {
                break;
            }
            let mut line = read_line(&mut reader, text)?;
            match line.kind {
                LineKind::Site => {
                    line.index = sites;
                    site = Some(sites);
                    sites = add(sites, 1);
                }
                LineKind::Refusal => {
                    line.site = site.ok_or(line.error(PatchWrong::Stray))?;
                    line.index = refusals;
                    refusals = add(refusals, 1);
                    for old in &parsed.lines {
                        if old.kind == LineKind::Refusal
                            && spelled(text, old.declaration) == spelled(text, line.declaration)
                        {
                            return Err(line.error(PatchWrong::Twice));
                        }
                    }
                }
                LineKind::Operation(_) | LineKind::Rename => {
                    site = None;
                }
            }
            parsed.lines.push(line).expect("each line consumed tokens");
        }
        parsed
            .blocks
            .push(Block { token, family, from, to, lines: Range { start, end: parsed.lines.len() } })
            .expect("each block consumed tokens");
    }
    parsed.tokens = reader.tokens;
    Ok(parsed)
}

fn part_allowed(kind: LineKind) -> bool {
    match kind {
        LineKind::Rename | LineKind::Site => true,
        LineKind::Refusal => false,
        LineKind::Operation(op) => match op {
            Op::Add | Op::Drop | Op::Widen | Op::Wrap | Op::Retype | Op::Payload => true,
            Op::Reshape | Op::Versioned | Op::Unversioned | Op::Order => false,
            Op::Written => unreachable!("written is a site line"),
        },
    }
}

fn read_line(reader: &mut Reader, text: &[u8]) -> Result<Line, PatchError> {
    let token = reader.take(text, Expected::Line)?;
    let kind = match spelled(text, token.at) {
        b"add" => LineKind::Operation(Op::Add),
        b"drop" => LineKind::Operation(Op::Drop),
        b"reshape" => LineKind::Operation(Op::Reshape),
        b"versioned" => LineKind::Operation(Op::Versioned),
        b"unversioned" => LineKind::Operation(Op::Unversioned),
        b"widen" => LineKind::Operation(Op::Widen),
        b"wrap" => LineKind::Operation(Op::Wrap),
        b"retype" => LineKind::Operation(Op::Retype),
        b"order" => LineKind::Operation(Op::Order),
        b"payload" => LineKind::Operation(Op::Payload),
        b"rename" => LineKind::Rename,
        b"written" => LineKind::Site,
        b"refuses" => LineKind::Refusal,
        _ => return Err(token.error(PatchWrong::Expected(Expected::Line))),
    };
    let declaration = reader.name(text, Expected::Name)?.at;
    let empty = Range { start: reader.at, end: reader.at };
    let mut line = Line {
        token,
        kind,
        declaration,
        part: None,
        other: declaration,
        old_type: empty,
        new_type: empty,
        value: empty,
        index: 0,
        site: 0,
    };
    if part_allowed(kind) && reader.is(text, b".")? {
        reader.take(text, Expected::Name)?;
        line.part = Some(reader.name(text, Expected::Name)?.at);
    }
    match kind {
        LineKind::Rename => {
            line.other = reader.name(text, Expected::Name)?.at;
        }
        LineKind::Refusal => {
            let token = reader.take(text, Expected::Text)?;
            if token.kind != Kind::Text {
                return Err(token.error(PatchWrong::Expected(Expected::Text)));
            }
            line.other = token.at;
        }
        LineKind::Site => {}
        LineKind::Operation(op) => match op {
            Op::Add => {
                if line.part.is_some() {
                    let next = reader.peek(text)?.expect("line has an end token");
                    if next.kind != Kind::End {
                        line.new_type = reader.ty(text)?;
                        line.value = reader.value(text)?;
                    }
                }
            }
            Op::Widen | Op::Wrap | Op::Retype | Op::Payload => {
                if line.part.is_none() {
                    return Err(reader.take(text, Expected::Name)?.error(PatchWrong::Expected(Expected::Name)));
                }
                if op == Op::Payload {
                    line.old_type = Range { start: reader.at, end: add(reader.at, 1) };
                    record(reader, text)?;
                    line.new_type = Range { start: reader.at, end: add(reader.at, 1) };
                    record(reader, text)?;
                } else {
                    line.old_type = reader.ty(text)?;
                    line.new_type = reader.ty(text)?;
                }
            }
            Op::Order => {
                let start = reader.at;
                reader.name(text, Expected::Name)?;
                for _ in 0..text.len() {
                    if reader.peek(text)?.expect("line has an end token").kind == Kind::End {
                        break;
                    }
                    reader.name(text, Expected::Name)?;
                }
                line.value = Range { start, end: reader.at };
            }
            Op::Reshape | Op::Versioned | Op::Unversioned | Op::Drop => {}
            Op::Written => unreachable!("written is a site line"),
        },
    }
    reader.end(text)?;
    Ok(line)
}
fn record(reader: &mut Reader, text: &[u8]) -> Result<(), PatchError> {
    if reader.is(text, b"-")? {
        reader.take(text, Expected::Record)?;
    } else {
        reader.name(text, Expected::Record)?;
    }
    Ok(())
}
