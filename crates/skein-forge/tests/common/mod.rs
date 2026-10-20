#![expect(
    clippy::disallowed_types,
    reason = "the document test harness retains finite fixture rows and mutation buffers in ordinary Rust collections"
)]
pub mod cases;
pub mod forgejo;
use skein_forge::forgejo::{document::User, limits::Limits};
use skein_lib::Rng;
#[path = "../../src/forgejo/tests/fixtures/admitted.rs"]
pub mod admitted;
#[path = "../fixtures.rs"]
pub mod fixtures;
pub fn mutated(rng: &mut Rng, golden: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    mutated_into(rng, golden, &mut bytes);
    bytes
}
pub fn mutated_into(rng: &mut Rng, golden: &[u8], bytes: &mut Vec<u8>) {
    bytes.clear();
    bytes.extend_from_slice(golden);
    let length = u64::try_from(bytes.len()).expect("a byte length fits");
    match rng.below(3) {
        0 if !bytes.is_empty() => {
            let at = usize::try_from(rng.below(length)).expect("a byte position fits");
            bytes[at] ^= 1 << rng.below(8);
        }
        1 if !bytes.is_empty() => {
            let at = usize::try_from(rng.below(length)).expect("a byte position fits");
            bytes.remove(at);
        }
        _ => {
            let at = usize::try_from(rng.between(0, length)).expect("a byte position fits");
            let byte = u8::try_from(rng.below(256)).expect("one byte");
            bytes.insert(at, byte);
        }
    }
}

pub mod generated;

pub mod goldens;
pub mod json;
