use crate::{Reason, piece};
use skein_lib::{List, Overflow, Reader, Writer};

fn write_items(items: &[&[u8]], size: usize) -> Box<[u8]> {
    let mut out = Writer::new(size);
    for item in items {
        piece::write(&mut out, item).expect("the fixture fits its writer");
    }
    out.finish()
}

fn split_items(bytes: &[u8]) -> Result<Box<[Box<[u8]>]>, Reason> {
    let mut reader = Reader::new(bytes);
    let mut items = List::with_capacity(4);
    while let Some(item) = piece::split(&mut reader)? {
        items.push(item).expect("at most four fixture items");
    }
    Ok(items.into_boxed())
}

#[test]
fn each_written_item_has_its_u32_length() {
    assert_eq!(write_items(&[b"ab", b"", b"c"], 15).as_ref(), &[0, 0, 0, 2, b'a', b'b', 0, 0, 0, 0, 0, 0, 0, 1, b'c']);
}

#[test]
fn an_item_that_exceeds_the_remaining_bound_writes_nothing() {
    let mut out = Writer::new(6);
    piece::write(&mut out, b"ab").expect("the first item fills the bound");
    assert_eq!(piece::write(&mut out, b""), Err(Overflow));
    assert_eq!(out.finish().as_ref(), &[0, 0, 0, 2, b'a', b'b']);
}

#[test]
fn an_item_header_that_exceeds_the_bound_writes_nothing() {
    let mut out = Writer::new(3);
    assert_eq!(piece::write(&mut out, b""), Err(Overflow));
    assert_eq!(out.written(), 0);
}

#[test]
fn an_item_body_that_exceeds_the_bound_writes_no_header() {
    let mut out = Writer::new(5);
    assert_eq!(piece::write(&mut out, b"ab"), Err(Overflow));
    assert_eq!(out.written(), 0);
}

#[test]
fn small_and_large_items_fit_the_same_sum_bound() {
    assert_eq!(write_items(&[b"a", b"b"], 10).as_ref(), &[0, 0, 0, 1, b'a', 0, 0, 0, 1, b'b']);
    assert_eq!(write_items(&[b"abcdef"], 10).as_ref(), &[0, 0, 0, 6, b'a', b'b', b'c', b'd', b'e', b'f']);
}

#[test]
fn splitting_ends_only_between_complete_items() {
    let items = split_items(&[0, 0, 0, 2, b'a', b'b', 0, 0, 0, 0, 0, 0, 0, 1, b'c']).expect("complete items");
    assert_eq!(items.as_ref(), &[Box::from(&b"ab"[..]), Box::from(&b""[..]), Box::from(&b"c"[..])]);
}

#[test]
fn an_empty_piece_has_no_items() {
    assert_eq!(split_items(b""), Ok(Box::from([])));
}

#[test]
fn an_item_length_past_the_end_is_malformed() {
    assert_eq!(split_items(&[0, 0, 0, 2, b'a']), Err(Reason::Malformed));
    assert_eq!(split_items(&[255, 255, 255, 255]), Err(Reason::Malformed));
}

#[test]
fn a_partial_item_length_at_the_tail_is_malformed() {
    for tail in [&b"\0"[..], &b"\0\0"[..], &b"\0\0\0"[..]] {
        assert_eq!(split_items(tail), Err(Reason::Malformed));
    }
    assert_eq!(split_items(&[0, 0, 0, 1, b'a', 0]), Err(Reason::Malformed));
}
