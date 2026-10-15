//! Total, bounded paths (ui-web.md, 9).
use alloc::boxed::Box;
use skein_lib::{Decimal, Writer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    List,
    Note {
        number: u64,
    },
    Missing,
    #[cfg(debug_assertions)]
    Panic,
}

#[must_use]
pub fn parse(path: &[u8]) -> Place {
    let end = match skein_lib::bytes::find(path, b"?") {
        Some(end) => end,
        None => path.len(),
    };
    let path = path.get(..end).expect("query begins within path");
    #[cfg(debug_assertions)]
    if path == b"/test/panic" {
        return Place::Panic;
    }
    #[cfg(debug_assertions)]
    if path == b"/test/small-store" {
        return Place::List;
    }
    if path == b"/" {
        return Place::List;
    }
    let Some(digits) = path.strip_prefix(b"/notes/") else {
        return Place::Missing;
    };
    if digits.len() > 20 || (digits.len() > 1 && digits.first() == Some(&b'0')) {
        return Place::Missing;
    }
    match skein_ui::number(digits) {
        Some(number) => Place::Note { number },
        None => Place::Missing,
    }
}

pub fn print(place: Place, out: &mut Writer) {
    let path: &[u8] = match place {
        Place::List => b"/",
        Place::Note { .. } => b"/notes/",
        Place::Missing => b"/missing",
        #[cfg(debug_assertions)]
        Place::Panic => b"/test/panic",
    };
    out.put(path).expect("path output reserves 27 bytes");
    match place {
        Place::Note { number } => out.put(Decimal::of(number).as_bytes()).expect("path output reserves 27 bytes"),
        Place::List | Place::Missing => {}
        #[cfg(debug_assertions)]
        Place::Panic => {}
    }
}

#[must_use]
pub fn bytes(place: Place) -> Box<[u8]> {
    let length = match place {
        Place::List => 1,
        Place::Missing => 8,
        #[cfg(debug_assertions)]
        Place::Panic => 11,
        Place::Note { number } => 7_usize.checked_add(Decimal::of(number).as_bytes().len()).expect("path fits"),
    };
    let mut out = Writer::new(length);
    print(place, &mut out);
    out.finish()
}
