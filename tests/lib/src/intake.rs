//! The intake's plain reference (lib.md, 7): what a demand takes from a
//! stream all of which is buffered, and a check of an intake fed in pieces
//! against it.

use skein_lib::Intake;
use skein_lib::Overflow;
use skein_lib::stream::{Delimiter, Read};

/// A scan for `until`, of at most `max` bytes.
#[must_use]
pub fn scan(until: Delimiter, max: u32) -> Read {
    Read::Scan { until, max }
}

/// What `read` takes from the front of `stream` when all of it is
/// buffered, found the plain way: how many bytes, or `None`.
#[must_use]
pub fn reference(stream: &[u8], read: Read) -> Option<usize> {
    match read {
        Read::Nothing => None,
        Read::Arrived { max } => {
            (!stream.is_empty()).then_some(stream.len().min(usize::try_from(max).expect("a u32 fits a usize")))
        }
        Read::Fill(n) => {
            let n = usize::try_from(n).expect("a u32 fits a usize");
            (stream.len() >= n).then_some(n)
        }
        Read::Scan { until, max } => {
            let max = usize::try_from(max).expect("a u32 fits a usize");
            let needle = until.as_bytes();
            let window = &stream[..stream.len().min(max)];
            for at in 0..window.len() {
                if window[at..].starts_with(needle) {
                    return Some(at.checked_add(needle.len()).expect("within the stream"));
                }
            }
            (stream.len() >= max).then_some(max)
        }
        Read::Line { max } => {
            let max = usize::try_from(max).expect("a u32 fits a usize");
            let window = &stream[..stream.len().min(max)];
            match window.iter().position(|&byte| byte == b'\r' || byte == b'\n') {
                Some(at) => Some(at + 1),
                None => (stream.len() >= max).then_some(max),
            }
        }
    }
}

/// Feeds `input` to an intake of `capacity` in pieces of the lengths in
/// `pieces` (cycled), cut further where a piece does not fit, meeting
/// `demands` in turn as soon as each can be met; checks every delivery
/// against the reference of what has arrived then, returning how many
/// demands are met.
#[expect(clippy::must_use_candidate, reason = "it asserts as it goes; the count is for a caller that knows it")]
pub fn check(input: &[u8], demands: &[Read], capacity: u32, pieces: &[usize]) -> usize {
    let mut intake = Intake::with_capacity(capacity);
    let (mut used, mut arrived) = (0, 0);
    let mut pieces = pieces.iter().cycle();
    for (index, &read) in demands.iter().enumerate() {
        loop {
            let held = &input[used..arrived];
            let expected = reference(held, read).map(|n| &held[..n]);
            let delivered = intake.meet(read);
            assert_eq!(delivered.as_deref(), expected, "demand {index}: {read:?}");
            if let Some(bytes) = delivered {
                used += bytes.len();
                break;
            }
            if read == Read::Nothing {
                break;
            }
            if arrived == input.len() {
                return index;
            }
            assert!(intake.room() > 0, "demand {index} ({read:?}) fits the cap, so a full intake meets it");
            let unfed = &input[arrived..];
            let want = (*pieces.next().expect("pieces cycle")).clamp(1, unfed.len());
            let fits = want.min(usize::try_from(intake.room()).expect("a u32 fits a usize"));
            if fits < want {
                let before = intake.len();
                assert_eq!(intake.append(&unfed[..want]), Err(Overflow));
                assert_eq!(intake.len(), before, "a refused append appends nothing");
            }
            assert_eq!(intake.append(&unfed[..fits]), Ok(()));
            arrived += fits;
        }
    }
    demands.len()
}

/// Takes `n` bytes from the front of the model, as a delivery does.
pub fn drain(model: &mut Vec<u8>, n: usize) {
    drop(model.drain(..n));
}
