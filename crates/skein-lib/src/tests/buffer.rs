//! Reusable bytes (lib.md, 6): capacity, back-patches, reuse and exact copies.

use crate::{Buffer, Overflow};

fn holding(capacity: u32, bytes: &[u8]) -> Buffer {
    let mut buffer = Buffer::with_capacity(capacity);
    buffer.put(bytes).expect("the fixture fits its capacity");
    buffer
}

#[test]
fn puts_fill_the_capacity_exactly() {
    // Capacities are the lengths of the complete records in each row.
    for (capacity, prefix, suffix, expected) in
        [(0, b"".as_slice(), b"".as_slice(), b"".as_slice()), (1, b"", b"a", b"a"), (4, b"ab", b"cd", b"abcd")]
    {
        let mut buffer = holding(capacity, prefix);
        assert_eq!(buffer.put(suffix), Ok(()));
        assert_eq!(buffer.as_slice(), expected);
        assert_eq!(buffer.len(), capacity);
        assert_eq!(buffer.capacity(), capacity);
        assert_eq!(buffer.room(), 0);
        assert_eq!(buffer.put(b""), Ok(()));
    }
}

#[test]
fn a_put_one_byte_past_capacity_writes_nothing() {
    // Each suffix makes the record one byte longer than its capacity.
    for (capacity, prefix, suffix) in [(0, b"".as_slice(), b"a".as_slice()), (4, b"ab", b"cde"), (4, b"abcd", b"e")] {
        let mut buffer = holding(capacity, prefix);
        let before = buffer.to_boxed();
        let len = buffer.len();
        assert_eq!(buffer.put(suffix), Err(Overflow));
        assert_eq!(buffer.len(), len);
        assert_eq!(buffer.as_slice(), before.as_ref());
    }
}

#[test]
fn back_patches_write_big_endian_counts_between_other_bytes() {
    const CAPACITY: u32 = 10; // Two prefix bytes, four count bytes, four item bytes.
    for (count, expected) in [(2, b"id\0\0\0\x02abcd".as_slice()), (0x0102_0304, b"id\x01\x02\x03\x04abcd")] {
        let mut buffer = holding(CAPACITY, b"id\0\0\0\0");
        buffer.put(b"abcd").expect("the items fit after the reserved count");
        buffer.set_u32(2, count);
        assert_eq!(buffer.as_slice(), expected);
        assert_eq!(buffer.len(), CAPACITY);
    }
}

#[test]
fn back_patches_can_end_at_the_last_byte_put() {
    for (bytes, at, expected) in
        [(b"\0\0\0\0".as_slice(), 0, b"\x01\x02\x03\x04".as_slice()), (b"a\0\0\0\0", 1, b"a\x01\x02\x03\x04")]
    {
        let capacity = u32::try_from(bytes.len()).expect("a small fixture");
        let mut buffer = holding(capacity, bytes);
        buffer.set_u32(at, 0x0102_0304);
        assert_eq!(buffer.as_slice(), expected);
    }
}

#[test]
#[should_panic(expected = "a back-patch writes over bytes already put")]
fn a_back_patch_one_byte_past_length_is_a_bug() {
    const CAPACITY: u32 = 5; // Room exists, but only four bytes have been put.
    let mut buffer = holding(CAPACITY, b"abcd");
    buffer.set_u32(1, 0);
}

#[test]
#[should_panic(expected = "a back-patch writes over bytes already put")]
fn a_back_patch_with_an_overflowing_end_is_a_bug() {
    let mut buffer = Buffer::with_capacity(0);
    buffer.set_u32(u32::MAX, 0);
}

#[test]
fn clearing_restores_room_for_a_second_record() {
    const CAPACITY: u32 = 6; // The first record fills the buffer.
    for second in [b"".as_slice(), b"new", b"second"] {
        let mut buffer = holding(CAPACITY, b"record");
        buffer.clear();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert_eq!(buffer.room(), CAPACITY);
        assert_eq!(buffer.capacity(), CAPACITY);
        buffer.put(second).expect("the second record fits");
        assert_eq!(buffer.to_boxed().as_ref(), second);
    }
}

#[test]
fn truncating_keeps_the_prefix_and_allocation_for_reuse() {
    const CAPACITY: u32 = 4; // The record abcd fills the buffer.
    for (len, expected, room, suffix, refilled) in [
        (0, b"".as_slice(), 4, b"xxxx".as_slice(), b"xxxx".as_slice()),
        (2, b"ab", 2, b"xx", b"abxx"),
        (4, b"abcd", 0, b"", b"abcd"),
    ] {
        let mut buffer = holding(CAPACITY, b"abcd");
        buffer.truncate(len);
        assert_eq!(buffer.as_slice(), expected);
        assert_eq!(buffer.len(), len);
        assert_eq!(buffer.room(), room);
        assert_eq!(buffer.capacity(), CAPACITY);
        buffer.put(suffix).expect("room");
        assert_eq!(buffer.as_slice(), refilled);
        assert_eq!(buffer.room(), 0);
    }
}

#[test]
fn truncating_past_length_changes_nothing() {
    const CAPACITY: u32 = 4; // Two bytes put, two bytes still available.
    for len in [3, CAPACITY, u32::MAX] {
        let mut buffer = holding(CAPACITY, b"ab");
        buffer.truncate(len);
        assert_eq!(buffer.as_slice(), b"ab");
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.room(), 2);
    }
}

#[test]
fn copying_out_keeps_exact_bytes_and_the_buffer() {
    const CAPACITY: u32 = 4; // Includes an empty, partial and full record.
    for bytes in [b"".as_slice(), b"ab", b"abcd"] {
        let mut buffer = holding(CAPACITY, bytes);
        let copy = buffer.to_boxed();
        assert_eq!(copy.as_ref(), bytes);
        assert_eq!(copy.len(), bytes.len());
        assert_eq!(buffer.as_slice(), bytes);
        assert_eq!(buffer.len(), u32::try_from(bytes.len()).expect("a small fixture"));
        assert_eq!(buffer.is_empty(), bytes.is_empty());
        buffer.clear();
        assert_eq!(copy.as_ref(), bytes);
    }
}

#[test]
fn worst_case_counts_the_capacity_in_bytes() {
    for (capacity, heap) in [(0, 0), (1, 1), (16, 16), (u32::MAX, 4_294_967_295)] {
        assert_eq!(Buffer::worst_case(capacity), Some(heap));
    }
}

#[test]
#[expect(clippy::disallowed_macros, reason = "the debug form is what is checked")]
fn debug_shows_lengths_never_bytes() {
    let buffer = holding(8, b"secret");
    assert_eq!(format!("{buffer:?}"), "Buffer { len: 6, capacity: 8 }");
}
