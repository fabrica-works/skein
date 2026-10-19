//! The response headers read beside Forgejo's documents and byte answers.
use crate::forgejo::{id, time};
use skein_lib::{Duration, Wall};
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ContentRange {
    pub first: u64,
    pub end: u64,
    pub total: Option<u64>,
}
#[must_use]
pub fn has_more(value: &[u8]) -> Option<bool> {
    match value {
        b"true" => Some(true),
        b"false" => Some(false),
        _ => None,
    }
}
#[must_use]
pub fn content_range(value: &[u8]) -> Option<ContentRange> {
    if value == b"bytes 0--1/0" {
        return Some(ContentRange { first: 0, end: 0, total: Some(0) });
    }
    let value = value.strip_prefix(b"bytes ")?;
    let dash = skein_lib::bytes::find(value, b"-")?;
    let slash = skein_lib::bytes::find(value, b"/")?;
    let first = id::unsigned(value.get(..dash)?)?;
    let last = id::unsigned(value.get(dash.checked_add(1)?..slash)?)?;
    let total = match value.get(slash.checked_add(1)?..)? {
        b"*" => None,
        text => Some(id::unsigned(text)?),
    };
    if first > last {
        return None;
    }
    if let Some(total) = total
        && last >= total
    {
        return None;
    }
    Some(ContentRange { first, end: last.checked_add(1)?, total })
}
#[must_use]
pub fn retry_after(value: &[u8], date: Wall) -> Option<Duration> {
    if let Some(seconds) = id::unsigned(value) {
        return Some(Duration::from_nanos(seconds.checked_mul(1_000_000_000)?));
    }
    let at = time::http_date(value)?;
    Some(Duration::from_nanos(at.as_nanos().saturating_sub(date.as_nanos())))
}
/// An Actions job link with the repository's absolute URL or origin-relative path
/// as its prefix; protocol-relative and other-origin links name no job.
#[must_use]
pub fn job_link(target_url: &[u8], html_url: &[u8]) -> Option<(u64, u32)> {
    if target_url.starts_with(b"//") {
        return None;
    }
    let suffix = if let Some(suffix) = target_url.strip_prefix(html_url) {
        suffix
    } else {
        let scheme = skein_lib::bytes::find(html_url, b"://")?;
        let authority = html_url.get(scheme.checked_add(3)?..)?;
        let start = skein_lib::bytes::find(authority, b"/")?;
        target_url.strip_prefix(authority.get(start..)?)?
    };
    let numbers = suffix.strip_prefix(b"/actions/runs/")?;
    let split = skein_lib::bytes::find(numbers, b"/jobs/")?;
    let run = id::unsigned(numbers.get(..split)?)?;
    let job = id::unsigned(numbers.get(split.checked_add(6)?..)?)?;
    Some((run, u32::try_from(job).ok()?))
}
