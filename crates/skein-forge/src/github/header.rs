//! The answer head read beside GitHub's documents (forge-github.md, section 2).
use super::limits::Limits;
use crate::{
    api,
    forgejo::{id::unsigned, time},
};
use alloc::boxed::Box;
use skein_lib::{Duration, Wall};
/// GitHub's rate window, present only when all three fields arrived.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RateFields {
    pub limit: u32,
    pub remaining: u32,
    pub reset: Wall,
}
/// The bucket to which the rate window belongs.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Resource {
    Core,
    Graphql,
    Search,
    Other,
}
/// A bounded printable delivery cursor.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Cursor(Box<[u8]>);
/// A bounded printable entity tag, including a weak prefix when present.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Etag(Box<[u8]>);
impl Cursor {
    /// Checks the 128-byte printable bound before copying.
    #[must_use]
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if printable(bytes) { Some(Self(skein_lib::bytes::copy_of(bytes))) } else { None }
    }
    /// The cursor's query value.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
impl Etag {
    /// Checks the 128-byte printable bound before copying.
    #[must_use]
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if printable(bytes) { Some(Self(skein_lib::bytes::copy_of(bytes))) } else { None }
    }
    /// The tag's HTTP spelling.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
fn printable(bytes: &[u8]) -> bool {
    if bytes.len() > 128 {
        return false;
    }
    for byte in bytes {
        if !(0x20..=0x7e).contains(byte) {
            return false;
        }
    }
    true
}
/// Fields accumulated in any HTTP header order.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[expect(
    clippy::partial_pub_fields,
    reason = "incomplete rate fields and an HTTP-date retry are retained until their companion headers arrive"
)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "next, cursor validity, JSON media and connection close are independent header facts"
)]
pub struct Head {
    pub date: Option<Wall>,
    pub retry_after: Option<Duration>,
    pub next: bool,
    pub cursor: Option<Cursor>,
    /// A log page cannot treat a malformed next cursor as the end of the log.
    pub(crate) cursor_invalid: bool,
    pub etag: Option<Etag>,
    pub location: Option<Box<[u8]>>,
    pub json: bool,
    pub rate: Option<RateFields>,
    pub resource: Option<Resource>,
    pub close: bool,
    limit: Option<u32>,
    remaining: Option<u32>,
    reset: Option<Wall>,
    retry_at: Option<Wall>,
}
impl Head {
    /// An answer before any header field.
    #[must_use]
    #[expect(
        clippy::new_without_default,
        reason = "a head starts explicitly before its first field; the programming model excludes Default"
    )]
    pub fn new() -> Self {
        Self {
            date: None,
            retry_after: None,
            next: false,
            cursor: None,
            cursor_invalid: false,
            etag: None,
            location: None,
            json: false,
            rate: None,
            resource: None,
            close: false,
            limit: None,
            remaining: None,
            reset: None,
            retry_at: None,
        }
    }
    /// The remaining quota, even when the rest of its window is absent.
    #[must_use]
    pub fn remaining(&self) -> Option<u32> {
        match self.rate {
            Some(rate) => Some(rate.remaining),
            None => self.remaining,
        }
    }
    pub(crate) fn reset(&self) -> Option<Wall> {
        match self.rate {
            Some(rate) => Some(rate.reset),
            None => self.reset,
        }
    }
}
/// Maps a complete GitHub rate window for the component's answer envelope.
#[must_use]
pub fn rate(head: &Head) -> Option<api::Rate> {
    let fields = head.rate?;
    Some(api::Rate { limit: fields.limit, remaining: fields.remaining, reset_at: fields.reset })
}
/// Checks an answer's entity tag against the vocabulary's validator bound.
#[must_use]
pub fn validator(head: &Head) -> Option<api::Validator> {
    api::Validator::new(head.etag.as_ref()?.as_bytes())
}
/// Reads one header field; unknown names are ignored.
pub fn field(head: &mut Head, name: &[u8], value: &[u8], limits: &Limits) {
    let raw_value = value;
    let value = value.trim_ascii();
    if name.eq_ignore_ascii_case(b"Date") {
        head.date = time::http_date(value);
    } else if name.eq_ignore_ascii_case(b"Retry-After") {
        head.retry_at = None;
        head.retry_after = match unsigned(value) {
            Some(seconds) => seconds_duration(seconds),
            None => {
                head.retry_at = time::http_date(value);
                None
            }
        };
    } else if name.eq_ignore_ascii_case(b"Link") {
        if let Some((next, cursor)) = link_next(value) {
            head.next |= next;
            if next {
                head.cursor_invalid |= cursor.is_none();
                head.cursor = cursor;
            }
        } else {
            head.cursor_invalid = true;
        }
    } else if name.eq_ignore_ascii_case(b"ETag") {
        head.etag = Etag::new(raw_value);
    } else if name.eq_ignore_ascii_case(b"Location") {
        head.location = if value.len() <= usize::try_from(limits.target_bytes).expect("u32 fits usize") {
            Some(skein_lib::bytes::copy_of(value))
        } else {
            None
        };
    } else if name.eq_ignore_ascii_case(b"Content-Type") {
        let (media, _) = split(value, b';');
        head.json = media.trim_ascii().eq_ignore_ascii_case(b"application/json");
    } else if name.eq_ignore_ascii_case(b"x-ratelimit-limit") {
        head.limit = number(value);
    } else if name.eq_ignore_ascii_case(b"x-ratelimit-remaining") {
        head.remaining = number(value);
    } else if name.eq_ignore_ascii_case(b"x-ratelimit-reset") {
        head.reset = seconds_wall(value);
    } else if name.eq_ignore_ascii_case(b"x-ratelimit-resource") {
        head.resource = Some(match value {
            b"core" => Resource::Core,
            b"graphql" => Resource::Graphql,
            b"search" => Resource::Search,
            _ => Resource::Other,
        });
    } else if name.eq_ignore_ascii_case(b"Connection") && contains_word(value, b',', b"close") {
        head.close = true;
    }
    if let Some(after) = retry_date(head.retry_at, head.date) {
        head.retry_after = Some(after);
    }
    head.rate = match (head.limit, head.remaining, head.reset) {
        (Some(limit), Some(remaining), Some(reset)) => Some(RateFields { limit, remaining, reset }),
        (None, Some(_) | None, Some(_) | None) | (Some(_), None, Some(_) | None) | (Some(_), Some(_), None) => None,
    };
}
fn number(value: &[u8]) -> Option<u32> {
    u32::try_from(unsigned(value)?).ok()
}
fn split(bytes: &[u8], delimiter: u8) -> (&[u8], &[u8]) {
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == delimiter {
            return (bytes.get(..i).expect("position"), bytes.get(i.saturating_add(1)..).expect("position"));
        }
    }
    (bytes, b"")
}
/// Reads rel="next" and that link's cursor; malformed link syntax has no next.
#[must_use]
pub fn link_next(value: &[u8]) -> Option<(bool, Option<Cursor>)> {
    let mut rest = value.trim_ascii();
    let mut found = false;
    let mut cursor = None;
    if rest.is_empty() {
        return None;
    }
    for _ in 0..=value.len() {
        let target = rest.strip_prefix(b"<")?;
        let at = skein_lib::bytes::find(target, b">")?;
        let url = target.get(..at)?;
        if url.is_empty() {
            return None;
        }
        for byte in url {
            if byte.is_ascii_control() || byte.is_ascii_whitespace() || *byte == b'<' {
                return None;
            }
        }
        rest = target.get(at.checked_add(1)?..)?.trim_ascii_start();
        let mut next = false;
        for _ in 0..=rest.len() {
            if !rest.starts_with(b";") {
                break;
            }
            rest = rest.get(1..)?.trim_ascii_start();
            let eq = skein_lib::bytes::find(rest, b"=")?;
            let name = rest.get(..eq)?.trim_ascii();
            if name.is_empty() || name.contains(&b';') || name.contains(&b',') {
                return None;
            }
            rest = rest.get(eq.checked_add(1)?..)?.trim_ascii_start();
            let parameter;
            if let Some(quoted) = rest.strip_prefix(b"\"") {
                let end = skein_lib::bytes::find(quoted, b"\"")?;
                parameter = quoted.get(..end)?;
                rest = quoted.get(end.checked_add(1)?..)?.trim_ascii_start();
            } else {
                let mut end = rest.len();
                for (i, byte) in rest.iter().enumerate() {
                    if *byte == b';' || *byte == b',' || byte.is_ascii_whitespace() {
                        end = i;
                        break;
                    }
                }
                parameter = rest.get(..end)?;
                rest = rest.get(end..)?.trim_ascii_start();
                if parameter.is_empty() {
                    return None;
                }
            }
            if name.eq_ignore_ascii_case(b"rel") && contains_word(parameter, b' ', b"next") {
                next = true;
            }
        }
        if next {
            found = true;
            cursor = link_cursor(url);
        }
        if rest.is_empty() {
            return Some((found, cursor));
        }
        rest = rest.strip_prefix(b",")?.trim_ascii_start();
        if rest.is_empty() {
            return None;
        }
    }
    None
}
fn link_cursor(url: &[u8]) -> Option<Cursor> {
    let query = url.get(skein_lib::bytes::find(url, b"?")?.checked_add(1)?..)?;
    let mut rest = query;
    for _ in 0..=query.len() {
        let (part, tail) = split(rest, b'&');
        rest = tail;
        let (name, value) = split(part, b'=');
        if name == b"cursor" {
            if value.len() > 384 {
                return None;
            }
            return Cursor::new(&super::request::unescape(value)?);
        }
        if rest.is_empty() {
            break;
        }
    }
    None
}

fn contains_word(bytes: &[u8], delimiter: u8, word: &[u8]) -> bool {
    let mut rest = bytes;
    for _ in 0..=bytes.len() {
        let (part, tail) = split(rest, delimiter);
        if part.trim_ascii().eq_ignore_ascii_case(word) {
            return true;
        }
        rest = tail;
        if rest.is_empty() {
            break;
        }
    }
    false
}

fn seconds_duration(seconds: u64) -> Option<Duration> {
    Some(Duration::from_nanos(seconds.checked_mul(1_000_000_000)?))
}
fn seconds_wall(bytes: &[u8]) -> Option<Wall> {
    Some(Wall::from_nanos(unsigned(bytes)?.checked_mul(1_000_000_000)?))
}

fn retry_date(at: Option<Wall>, date: Option<Wall>) -> Option<Duration> {
    Some(Duration::from_nanos(at?.as_nanos().saturating_sub(date?.as_nanos())))
}
