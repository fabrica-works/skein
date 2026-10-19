//! Public Responses request rules (fake-llm.md, 3.1; llm.md, 4.8).
use crate::documents::Error;
use skein_json::{Document, Kind};

pub(super) fn reserved(name: &[u8]) -> bool {
    name.eq_ignore_ascii_case(b"session-id") || name.eq_ignore_ascii_case(b"thread-id")
}

const REFUSED: [&[u8]; 15] = [
    b"background",
    b"conversation",
    b"max_output_tokens",
    b"max_tool_calls",
    b"metadata",
    b"moderation",
    b"multi_agent",
    b"prompt",
    b"prompt_cache_retention",
    b"safety_identifier",
    b"temperature",
    b"top_logprobs",
    b"top_p",
    b"truncation",
    b"user",
];

pub(crate) fn fields(document: &Document) -> Result<(), Error> {
    let mut depth = 0_u32;
    for index in 0..document.len() {
        let token = document.token(index).ok_or(Error::Malformed)?;
        match token.kind {
            Kind::ObjectStart | Kind::ArrayStart => depth = depth.checked_add(1).ok_or(Error::TooLarge)?,
            Kind::ObjectEnd | Kind::ArrayEnd => depth = depth.checked_sub(1).ok_or(Error::Malformed)?,
            Kind::Key => {
                if depth == 1 {
                    for field in REFUSED {
                        if document.text(token) == Some(field) {
                            return Err(Error::Refused(field));
                        }
                    }
                }
            }
            Kind::String | Kind::Number | Kind::True | Kind::False | Kind::Null | Kind::Long => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::reserved;

    #[test]
    fn the_public_route_refuses_session_and_thread_headers() {
        for name in [&b"session-id"[..], b"Thread-Id", b"THREAD-ID"] {
            assert!(reserved(name), "{name:?}");
        }
        assert!(!reserved(b"x-request-id"));
    }
}
