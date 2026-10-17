use core::hash::Hasher;

use super::sha256::hex;
use crate::{StateHasher, state_digest};

#[test]
fn a_u16_digests_as_its_two_little_endian_bytes() {
    // printf '\002\001' | sha256sum
    let printed = hex("25dfd29c09617dcc9852281c030e5b3037a338a4712a42a21c907f259c6412a0");
    let mut hasher = StateHasher::new();
    hasher.write_u16(0x0102);
    assert_eq!(hasher.digest(), printed, "low byte first, at the integer's own width");
    assert_eq!(state_digest(&0x0102_u16), printed, "a value hashed whole writes the same");
}

#[test]
fn a_usize_digests_as_the_eight_bytes_of_its_u64() {
    // printf '\001\000\000\000\000\000\000\000' | sha256sum
    let printed = hex("7c9fa136d4413fa6173637e883b6998d32e1d675f88cddff9dcbcf331820f4b8");
    assert_eq!(state_digest(&1_usize), printed);
    assert_eq!(state_digest(&1_u64), printed);
}

#[test]
fn a_negative_isize_digests_as_the_eight_bytes_of_its_i64() {
    // printf '\377\377\377\377\377\377\377\377' | sha256sum
    let printed = hex("12a3ae445661ce5dee78d0650d33362dec29c4f82af05e7e57fb595bbbacf0ca");
    assert_eq!(state_digest(&-1_isize), printed);
    assert_eq!(state_digest(&-1_i64), printed);
}

#[test]
fn bytes_written_in_pieces_digest_as_written_once() {
    // printf 'abc' | sha256sum
    let printed = hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    let mut pieces = StateHasher::new();
    pieces.write(b"a");
    pieces.write(b"bc");
    let mut whole = StateHasher::new();
    whole.write(b"abc");
    assert_eq!(pieces.digest(), printed);
    assert_eq!(whole.digest(), printed);
}

#[test]
fn finish_gives_the_digest_s_first_eight_bytes_without_ending_it() {
    let mut hasher = StateHasher::new();
    hasher.write_u16(0x0102);
    assert_eq!(hasher.finish(), 0x25df_d29c_0961_7dcc);
    assert_eq!(hasher.finish(), 0x25df_d29c_0961_7dcc);
    hasher.write_u8(3);
    // printf '\002\001\003' | sha256sum
    assert_eq!(hasher.digest(), hex("3ddef33d0729aa4cfb41361985817c9c0e4dca1a67f794530a52716cfcebd60d"));
}

#[test]
fn a_tuple_hashed_whole_digests_its_fields_in_order() {
    // printf '\001\002\000\000\000' | sha256sum
    assert_eq!(state_digest(&(1_u8, 2_u32)), hex("b0b8ad02f2f05ebcc80d37e5324dbf0d88cfd6cb808676f6d7dc5d0b6d3286fa"));
}
