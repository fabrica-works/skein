//! Borrowed reads of one retained JSON document and bounded two-pass writes.
use crate::Result;
use skein_json::{Document, Kind, collector, tokenizer, writer};
use skein_lib::{
    Env, Intake, Queue, Time, Wall,
    stream::{Down, Up},
};
use writer::Encoder;

/// One parsed document, retained whole; reads borrow from it.
pub struct Json(Document);
impl Json {
    /// # Errors
    /// Returns malformed JSON or a document beyond the tokenizer's limits.
    pub fn parse(input: &[u8]) -> Result<Self> {
        let length = u32::try_from(input.len())?;
        let limits = collector::Limits {
            tokenizer: tokenizer::Limits { depth: 32, string: length, number: length, chunk: length.max(1), length },
            tokens: length,
            text: length,
            skip: u64::from(length),
        };
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        let mut machine = collector::Collector::new(collector::Filter { root: collector::Keep::Value }, &limits, &[])
            .map_err(|error| format!("{error:?}"))?;
        let mut above = Queue::with_capacity(1);
        let mut below = Queue::with_capacity(1);
        let mut intake = Intake::with_capacity(length.max(tokenizer::largest_demand(&limits.tokenizer)));
        intake.append(input).map_err(|error| format!("{error:?}"))?;
        collector::down(&mut machine, &env, collector::Request::Collect, &mut above, &mut below);
        loop {
            if let Some(event) = above.pop() {
                return match event {
                    collector::Event::Collected(document) => Ok(Self(document)),
                    collector::Event::Failed(error) => Err(format!("{error:?}").into()),
                    collector::Event::Closed => Err("collector closed".into()),
                };
            }
            let event = match below.pop().ok_or("collector did not demand input")? {
                Down::Demand { read, room: 0 } => intake.meet(read).map_or(Up::End, Up::Bytes),
                Down::Demand { .. } | Down::Send(_) | Down::Finish => return Err("unexpected collector demand".into()),
            };
            collector::up(&mut machine, &env, event, &mut above, &mut below);
        }
    }
    #[must_use]
    pub fn root(&self) -> At<'_> {
        At { document: &self.0, position: Some(0) }
    }
    #[must_use]
    pub fn has_key(&self, name: &str) -> bool {
        (0..self.0.len()).any(|index| {
            self.0
                .token(index)
                .is_some_and(|token| token.kind == Kind::Key && self.0.text(token) == Some(name.as_bytes()))
        })
    }
}

/// A position in a `Json`; a wrong path or kind reads as `None`.
#[derive(Clone, Copy)]
pub struct At<'a> {
    document: &'a Document,
    position: Option<u32>,
}
impl<'a> At<'a> {
    fn at(self, position: Option<u32>) -> Self {
        Self { position, ..self }
    }
    pub(crate) fn kind(self) -> Option<Kind> {
        Some(self.document.token(self.position?)?.kind)
    }
    fn text(self, kind: Kind) -> Option<&'a str> {
        let token = self.document.token(self.position?)?;
        (token.kind == kind).then(|| std::str::from_utf8(self.document.text(token)?).ok()).flatten()
    }
    fn after(self) -> Option<u32> {
        let mut index = self.position?;
        let mut depth = 0_u32;
        loop {
            match self.document.token(index)?.kind {
                Kind::ObjectStart | Kind::ArrayStart => depth += 1,
                Kind::ObjectEnd | Kind::ArrayEnd => depth -= 1,
                Kind::Key | Kind::String | Kind::Number | Kind::True | Kind::False | Kind::Null | Kind::Long => {}
            }
            index += 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    #[must_use]
    pub fn get(self, name: &str) -> Self {
        if self.kind() != Some(Kind::ObjectStart) {
            return self.at(None);
        }
        let mut index = self.position.map(|position| position + 1);
        let mut found = None;
        while let Some(key) = index {
            let at = self.at(Some(key));
            if at.kind() != Some(Kind::Key) {
                break;
            }
            let value = self.at(Some(key + 1));
            if at.text(Kind::Key) == Some(name) {
                found = value.position;
            }
            index = value.after();
        }
        self.at(found)
    }
    #[must_use]
    pub fn items(self) -> Vec<Self> {
        let mut items = Vec::new();
        if self.kind() == Some(Kind::ArrayStart) {
            let mut index = self.position.map(|position| position + 1);
            while let Some(position) = index {
                let at = self.at(Some(position));
                if at.kind() == Some(Kind::ArrayEnd) {
                    break;
                }
                items.push(at);
                index = at.after();
            }
        }
        items
    }
    pub(crate) fn array(self) -> Option<Vec<Self>> {
        (self.kind() == Some(Kind::ArrayStart)).then(|| self.items())
    }
    #[must_use]
    pub fn nth(self, n: usize) -> Self {
        self.items().get(n).copied().unwrap_or(self.at(None))
    }
    #[must_use]
    pub fn str(self) -> Option<&'a str> {
        self.text(Kind::String)
    }
    #[must_use]
    pub fn u64(self) -> Option<u64> {
        self.text(Kind::Number)?.parse().ok()
    }
    #[must_use]
    pub fn bool(self) -> Option<bool> {
        match self.kind() {
            Some(Kind::True) => Some(true),
            Some(Kind::False) => Some(false),
            _ => None,
        }
    }
    #[must_use]
    pub fn is(self, text: &str) -> bool {
        self.str() == Some(text)
    }
    #[must_use]
    pub fn same(self, other: At<'_>) -> bool {
        std::ptr::eq(self.document, other.document) && self.position.is_some() && self.position == other.position
    }
}

/// A value the encoder can place without an intermediate value tree.
pub trait Put {
    fn put(&self, out: &mut Encoder);
}
impl Put for str {
    fn put(&self, out: &mut Encoder) {
        out.string(self.as_bytes());
    }
}
impl Put for String {
    fn put(&self, out: &mut Encoder) {
        self.as_str().put(out);
    }
}
impl Put for bool {
    fn put(&self, out: &mut Encoder) {
        out.boolean(*self);
    }
}
impl Put for u64 {
    fn put(&self, out: &mut Encoder) {
        out.unsigned(*self);
    }
}
impl<T: Put + ?Sized> Put for &T {
    fn put(&self, out: &mut Encoder) {
        (*self).put(out);
    }
}
impl<T: Put> Put for [T] {
    fn put(&self, out: &mut Encoder) {
        out.array_start();
        for value in self {
            value.put(out);
        }
        out.array_end();
    }
}
impl<T: Put, const N: usize> Put for [T; N] {
    fn put(&self, out: &mut Encoder) {
        self.as_slice().put(out);
    }
}
/// Writes a compact object, with strictly increasing member names.
/// # Errors
/// Returns unordered or repeated keys, or a document beyond the writer's limits.
pub fn object(members: &[(&str, &dyn Put)]) -> Result<Box<[u8]>> {
    if members.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err("JSON object keys must be sorted and unique".into());
    }
    write(|out| {
        out.object_start();
        for (key, value) in members {
            out.key(key.as_bytes());
            value.put(out);
        }
        out.object_end();
    })
}

/// Runs the builder twice, measuring before allocating the encoded document.
/// # Errors
/// Returns a document beyond the writer's limits.
pub fn write(build: impl Fn(&mut Encoder)) -> Result<Box<[u8]>> {
    write_bounded(65_536, build)
}
/// Runs both passes under the caller's document-length bound, at depth eight.
/// # Errors
/// Returns a document beyond the writer's limits.
pub fn write_bounded(length: u32, build: impl Fn(&mut Encoder)) -> Result<Box<[u8]>> {
    let limits = writer::Limits { depth: 8, length };
    let mut measure = Encoder::measure(&limits);
    build(&mut measure);
    let length = measure.measured().map_err(|error| format!("{error:?}"))?;
    let mut out = Encoder::write(length, &limits);
    build(&mut out);
    Ok(out.finish())
}
#[cfg(test)]
mod tests;
