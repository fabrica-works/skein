//! Name comparisons and points used by browser operations.

use alloc::boxed::Box;

use crate::boundary::{Match, Query};
use crate::wire::decode::Value;

/// The query's bytes occur unbroken, with case kept.
pub(crate) fn words(name: &[u8], query: &[u8]) -> bool {
    if query.is_empty() {
        return true;
    }
    for part in name.windows(query.len()) {
        if part == query {
            return true;
        }
    }
    false
}

pub(crate) fn accepts(node: Value<'_>, query: &Query) -> bool {
    match query.matching {
        Match::Exact => true,
        Match::Words => {
            let name = full_name(node);
            words(name.as_deref().unwrap_or_default(), &query.name)
        }
    }
}

fn full_name(node: Value<'_>) -> Option<Box<[u8]>> {
    node.get(b"name")?.get(b"value")?.text()
}

#[cfg(test)]
mod tests {
    use super::words;

    const CASES: &[(&[u8], bool)] = &[(b"3 failing", true), (b"3 passed", false), (b"Failing", false), (b"", true)];

    #[test]
    fn words_match_a_name_holding_them_unbroken() {
        for (query, expected) in CASES {
            assert_eq!(words(b"Checks: 3 failing, 2 passed", query), *expected, "query {query:?}");
        }
    }
}
