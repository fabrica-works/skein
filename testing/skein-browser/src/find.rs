//! Name comparisons and points used by browser operations.

use alloc::boxed::Box;

use crate::boundary::{Match, Point, Query, Rect};
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

/// The box's right and bottom edges are outside it.
pub(crate) fn point_in(rect: Rect, at: Point) -> bool {
    u64::from(at.x) < rect.width && u64::from(at.y) < rect.height
}

pub(crate) fn position(rect: Rect, at: Option<Point>) -> Option<(i64, i64)> {
    if rect.width == 0 || rect.height == 0 {
        return None;
    }
    let (x, y) = match at {
        Some(point) => {
            if !point_in(rect, point) {
                return None;
            }
            (i64::from(point.x), i64::from(point.y))
        }
        None => (i64::try_from(rect.width / 2).unwrap_or(i64::MAX), i64::try_from(rect.height / 2).unwrap_or(i64::MAX)),
    };
    Some((rect.x.saturating_add(x), rect.y.saturating_add(y)))
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
