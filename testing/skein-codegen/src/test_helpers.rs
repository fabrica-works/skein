//! Check every case and name failures without allocating diagnostic strings.

use alloc::boxed::Box;

type Case = (&'static str, fn() -> bool);

pub(super) fn table<const N: usize>(cases: &[Case; N]) {
    let mut failures = [None; N];
    let mut passed = true;
    for ((name, case), failure) in cases.iter().zip(&mut failures) {
        if !case() {
            *failure = Some(name);
            passed = false;
        }
    }
    assert!(passed, "{failures:?}");
}

pub(super) fn golden<T: PartialEq, E, D>(
    value: T,
    bytes: &[u8],
    encoded: Result<Box<[u8]>, E>,
    decoded: Result<T, D>,
) -> bool {
    let encoded = match encoded {
        Ok(encoded) => encoded.as_ref() == bytes,
        Err(_) => false,
    };
    let decoded = match decoded {
        Ok(decoded) => decoded == value,
        Err(_) => false,
    };
    encoded && decoded
}

type BoundDecode<T, L, P, R> = fn(&L, &[u8]) -> Result<T, (P, R)>;

pub(super) fn bound<T, L, P: PartialEq, R: PartialEq>(
    label: (P, R),
    limits: &L,
    at: &[u8],
    past: &[u8],
    decode: BoundDecode<T, L, P, R>,
) -> bool {
    let (path, reason) = label;
    let at = decode(limits, at).is_ok();
    let past = match decode(limits, past) {
        Err((found_path, found_reason)) => found_path == path && found_reason == reason,
        Ok(_) => false,
    };
    at && past
}
