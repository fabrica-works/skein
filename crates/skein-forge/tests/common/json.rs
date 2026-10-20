#![expect(
    clippy::arithmetic_side_effects,
    reason = "fixture lengths and seeds are bounded by the literal documents and the prescribed seed range"
)]
#![expect(
    clippy::default_numeric_fallback,
    reason = "the copied bounded sweeps retain their original counts and workloads"
)]
#![expect(clippy::disallowed_macros, reason = "the document harness formats fixture values and literal expected rows")]
#![expect(
    clippy::disallowed_methods,
    reason = "the document harness transforms finite fixture rows with ordinary Rust iterators"
)]
#![expect(
    clippy::must_use_candidate,
    reason = "fixture-returning helpers are only consumed by assertions or other fixture construction"
)]
#![expect(
    clippy::std_instead_of_alloc,
    reason = "the integration harness runs under std and retains the copied fixture imports"
)]
use super::forgejo::{Kind, drive, limits};
use skein_json::tokenizer;
use std::collections::BTreeSet;
/// Collect JSON with the same bounded tokenizer used by the dialect.
pub fn tokens(bytes: &[u8]) -> Option<Vec<skein_json::Token>> {
    let mut tokens = Vec::with_capacity(bytes.len());
    let mut valid = false;
    drive(bytes, &limits(), tokenizer::Request::Next, |event| match event {
        tokenizer::Event::Token(token) => {
            tokens.push(token.clone());
            Some(tokenizer::Request::Next)
        }
        tokenizer::Event::Done => {
            valid = true;
            None
        }
        tokenizer::Event::Failed(_) => None,
        tokenizer::Event::Long(_) | tokenizer::Event::Skipped(_) | tokenizer::Event::Closed => {
            panic!("uncapped shape string")
        }
    });
    if !valid {
        return None;
    }
    Some(tokens)
}
#[must_use]
pub fn shape(bytes: &[u8]) -> BTreeSet<(String, Kind)> {
    use skein_json::Token;
    let tokens = tokens(bytes).expect("JSON shape");
    let mut paths = vec![String::new()];
    let mut path = String::new();
    let mut set = BTreeSet::new();
    for token in tokens {
        let kind = match token {
            Token::Key(key) => {
                let key = core::str::from_utf8(&key).expect("UTF-8");
                let parent = paths.last().expect("parent");
                path = if parent.is_empty() { key.to_string() } else { format!("{parent}.{key}") };
                continue;
            }
            Token::ObjectEnd | Token::ArrayEnd => {
                paths.pop().expect("container");
                path.clone_from(paths.last().expect("root path"));
                continue;
            }
            Token::ObjectStart => Kind::Object,
            Token::ArrayStart => Kind::Array,
            Token::Number(_) => Kind::Number,
            Token::String(_) => Kind::String,
            Token::True | Token::False => Kind::Bool,
            Token::Null => Kind::Null,
        };
        set.insert((path.clone(), kind));
        match kind {
            Kind::Object => paths.push(path.clone()),
            Kind::Array => {
                path = format!("{path}[]");
                paths.push(path.clone());
            }
            Kind::Number | Kind::String | Kind::Bool | Kind::Null => {}
        }
        if let Some(parent) = paths.last() {
            path.clone_from(parent);
        }
    }
    set
}

/// Edit one parsed field record, retaining the other JSON records unchanged.
pub fn replace(body: &[u8], before: &[u8], after: &[u8]) -> Vec<u8> {
    let mut body = tokens(body).expect("recorded JSON");
    let fragment = |bytes: &[u8]| {
        let bytes = bytes.strip_prefix(b",").unwrap_or(bytes);
        let bytes = bytes.strip_suffix(b",").unwrap_or(bytes);
        let raw = [b"{".as_slice(), bytes, b"}"].concat();
        let tokens = tokens(&raw).expect("literal field records");
        tokens[1..tokens.len() - 1].to_vec()
    };
    let before = fragment(before);
    let after = fragment(after);
    let mut edits = 0;
    for index in 0..body.len() {
        if body[index..].starts_with(&before) {
            body.splice(index..index + before.len(), after);
            edits += 1;
            break;
        }
    }
    assert_eq!(edits, 1, "one intended JSON field edited");
    let limits = limits().writer;
    let mut measure = skein_json::writer::Encoder::measure(&limits);
    for token in &body {
        measure.token(token);
    }
    let length = measure.measured().expect("mutation size");
    let mut write = skein_json::writer::Encoder::write(length, &limits);
    for token in &body {
        write.token(token);
    }
    write.finish().into_vec()
}
