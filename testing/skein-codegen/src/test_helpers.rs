//! Collect every failing case before a table makes its one assertion.

use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt::Debug;

pub(super) fn golden<T: Debug + PartialEq, P: Debug, E: Debug, D: Debug>(
    name: &str,
    path: P,
    value: T,
    bytes: &[u8],
    encoded: Result<Box<[u8]>, E>,
    decoded: Result<T, D>,
    failures: &mut Vec<String>,
) {
    match encoded {
        Ok(encoded) if encoded.as_ref() == bytes => {}
        other => failures.push(format!("{name} ({path:?}): encoding {other:?}, expected {bytes:?}")),
    }
    match decoded {
        Ok(decoded) if decoded == value => {}
        other => failures.push(format!("{name} ({path:?}): decoding {other:?}, expected {value:?}")),
    }
}

type BoundDecode<T, L, P, R> = fn(&L, &[u8]) -> Result<T, (P, R)>;

pub(super) fn bound<T, L, P: Debug + PartialEq, R: Debug + PartialEq>(
    label: (&str, P, R),
    limits: &L,
    at: &[u8],
    past: &[u8],
    decode: BoundDecode<T, L, P, R>,
    failures: &mut Vec<String>,
) {
    let (name, path, reason) = label;
    if let Err(problem) = decode(limits, at) {
        failures.push(format!("{name} ({path:?}): at limit refused as {problem:?}"));
    }
    match decode(limits, past) {
        Err((found_path, found_reason)) if found_path == path && found_reason == reason => {}
        Err(problem) => failures.push(format!("{name} ({path:?}): past limit refused as {problem:?}")),
        Ok(_) => failures.push(format!("{name} ({path:?}): past limit accepted")),
    }
}
