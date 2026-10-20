use super::{Limits, User};
use skein_json::tokenizer;
// The sampled corpus's largest document is pull_files_decode_forgejos_changed_status_as_modified (347 bytes).
const DOCUMENT_BYTES: u32 = 347;
// The longest retained text is MergeProtected's canned refusal (61 bytes).
const STRING_BYTES: u32 = 61;
// Eight containers are the dialect's minimum accepted depth (limits::check).
const JSON_DEPTH: u32 = 8;
// The malformed-id fixture must tokenize both bytes of -1 before its range check.
const NUMBER_BYTES: u32 = 2;
// The comparison target with files=false and verification=false has 153 bytes.
const TARGET_BYTES: u32 = 153;
// Ordinary nested lists hold at most one item; the two-item boundary tests choose inner explicitly.
const INNER_ITEMS: u32 = 1;
// These are the sampled boundary strings' exact lengths, independent of shipped profiles.
pub const SMALL_STRING_BYTES: u32 = 8;
pub const SMALL_BODY_BYTES: u32 = 16;
#[must_use]
pub fn limits() -> Limits {
    Limits {
        json: tokenizer::Limits {
            depth: JSON_DEPTH,
            string: STRING_BYTES,
            number: NUMBER_BYTES,
            chunk: STRING_BYTES,
            length: DOCUMENT_BYTES,
        },
        writer: skein_json::writer::Limits { depth: JSON_DEPTH, length: DOCUMENT_BYTES },
        string_bytes: STRING_BYTES,
        body_bytes: SMALL_BODY_BYTES,
        inner: INNER_ITEMS,
        target_bytes: TARGET_BYTES,
    }
}
#[must_use]
pub fn small() -> Limits {
    Limits { string_bytes: SMALL_STRING_BYTES, body_bytes: SMALL_BODY_BYTES, ..limits() }
}
/// Shipped Small profile document bounds, independent of any service.
#[must_use]
pub fn profile_limits() -> Limits {
    Limits {
        json: tokenizer::Limits { depth: 16, string: 16_384, number: 32, chunk: 4_096, length: 16_777_216 },
        writer: skein_json::writer::Limits { depth: 16, length: 16_777_216 },
        string_bytes: 512,
        body_bytes: 16_384,
        inner: 32,
        target_bytes: 4_096,
    }
}

#[must_use]
pub fn user(id: u64, login: &[u8]) -> User {
    User { id, login: Box::from(login), is_admin: false, full_name: Box::from([]) }
}
