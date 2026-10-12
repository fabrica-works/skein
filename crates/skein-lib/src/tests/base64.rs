//! RFC 4648 vectors and canonical refusals (lib.md, 6.1).
use crate::{
    Overflow, Writer,
    base64::{self, Alphabet, Decoder, Error},
};

const VECTORS: [(&[u8], &[u8]); 7] = [
    (b"", b""),
    (b"f", b"Zg=="),
    (b"fo", b"Zm8="),
    (b"foo", b"Zm9v"),
    (b"foob", b"Zm9vYg=="),
    (b"fooba", b"Zm9vYmE="),
    (b"foobar", b"Zm9vYmFy"),
];
#[test]
fn rfc_4648_vectors_encode_in_the_standard_alphabet_with_padding() {
    for (bytes, text) in VECTORS {
        let len = base64::encoded_len(Alphabet::Standard, u32::try_from(bytes.len()).expect("short"));
        let mut writer = Writer::new(usize::try_from(len.expect("fits")).expect("fits"));
        base64::encode(Alphabet::Standard, bytes, &mut writer).expect("measured room");
        assert_eq!(writer.finish().as_ref(), text, "RFC 4648, section 10");
    }
}
#[test]
fn rfc_4648_vectors_decode_back_in_the_standard_alphabet() {
    for (bytes, text) in VECTORS {
        assert_eq!(base64::decode(Alphabet::Standard, text, 6).expect("canonical").as_ref(), bytes);
        assert_eq!(base64::decoded_len(Alphabet::Standard, text), Ok(u32::try_from(bytes.len()).expect("short")));
    }
}
#[test]
fn the_url_alphabet_encodes_without_padding() {
    let mut writer = Writer::new(3);
    base64::encode(Alphabet::Url, &[0xfb, 0xff], &mut writer).expect("room");
    assert_eq!(writer.finish().as_ref(), b"-_8");
    assert_eq!(base64::decode(Alphabet::Url, b"-_8", 2).expect("canonical").as_ref(), &[0xfb, 0xff]);
}
#[test]
fn decode_refuses_a_symbol_outside_the_alphabet_at_its_offset() {
    assert_eq!(base64::decode(Alphabet::Url, b"Zm!v", 3), Err(Error::Symbol { at: 2 }));
    assert_eq!(base64::decode(Alphabet::Standard, b"-_8=", 2), Err(Error::Symbol { at: 0 }));
}
#[test]
fn decode_refuses_padding_in_the_url_alphabet() {
    assert_eq!(base64::decode(Alphabet::Url, b"Zg==", 1), Err(Error::Padding { at: 2 }));
}
#[test]
fn decode_refuses_padding_anywhere_but_the_end() {
    assert_eq!(base64::decode(Alphabet::Standard, b"Zg=A", 3), Err(Error::Padding { at: 3 }));
    assert_eq!(base64::decode(Alphabet::Standard, b"Zg===", 3), Err(Error::Padding { at: 4 }));
    assert_eq!(base64::decode(Alphabet::Standard, b"Zm9v=", 3), Err(Error::Padding { at: 4 }));
}
#[test]
fn decode_refuses_a_length_no_encoding_has() {
    assert_eq!(base64::decode(Alphabet::Url, b"Z", 1), Err(Error::Length));
    assert_eq!(base64::decode(Alphabet::Standard, b"Zg", 1), Err(Error::Length));
    assert_eq!(base64::decode(Alphabet::Standard, b"Zg=", 1), Err(Error::Length));
}
#[test]
fn decode_refuses_trailing_bits_that_are_not_zero() {
    assert_eq!(base64::decode(Alphabet::Url, b"Zh", 1), Err(Error::Bits));
    assert_eq!(base64::decode(Alphabet::Standard, b"Zm9=", 2), Err(Error::Bits));
}
#[test]
fn decode_refuses_a_text_past_its_maximum() {
    assert_eq!(base64::decode(Alphabet::Standard, b"Zm9v", 2), Err(Error::TooLong { max: 2 }));
    let mut decoder = Decoder::new(Alphabet::Standard, 2);
    assert_eq!(decoder.push(b"Zm9v"), Err(Error::TooLong { max: 2 }));
    assert!(decoder.is_empty(), "a refused piece writes nothing");
    assert_eq!(decoder.finish(), Err(Error::TooLong { max: 2 }));
}
#[test]
fn pieces_cut_at_each_offset_decode_as_one_text() {
    for (bytes, text) in VECTORS {
        for split in 0..=text.len() {
            let mut decoder = Decoder::new(Alphabet::Standard, 6);
            decoder.push(&text[..split]).expect("first piece");
            decoder.push(&text[split..]).expect("second piece");
            assert_eq!(decoder.finish().expect("whole text").as_ref(), bytes);
        }
    }
}
#[test]
fn an_encoding_that_does_not_fit_writes_nothing() {
    let mut writer = Writer::new(2);
    assert_eq!(base64::encode(Alphabet::Standard, b"f", &mut writer), Err(Overflow));
    assert_eq!(writer.written(), 0);
    assert_eq!(base64::encoded_len(Alphabet::Standard, u32::MAX), None);
}
