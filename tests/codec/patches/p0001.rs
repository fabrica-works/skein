//! Written conversions for patch 0001 (codec.md, 11.5).

use super::Context;
use super::kept::every::{v1, v2};
use skein_codec::patch::{Build, Halt, Old};
use skein_codec::value::Node;

/// Site 0, `written Shape`, 1 to 2: dot becomes a versioned Blank;
/// every other variant carries through its derived conversion.
pub fn shape(old: &Old, new: &mut Build, at: Node, _cx: &mut Context) -> Result<(), Halt> {
    if old.tag(at)? != v1::shape::DOT {
        return new.carry(old, at);
    }
    new.open_variant(v2::shape::BLANK)?;
    new.close()
}

/// Site 1, `written Every.score`, 1 to 2: small plus short from the
/// old Every's numbers, read by field constants.
pub fn every_score(old: &Old, new: &mut Build, at: Node, _cx: &mut Context) -> Result<(), Halt> {
    let numbers = old.field(at, v1::every::NUMBERS)?;
    let small = old.uint(old.field(numbers, v1::numbers::SMALL)?)?;
    let short = old.uint(old.field(numbers, v1::numbers::SHORT)?)?;
    new.put_uint(small.checked_add(short).expect("u8 plus u16 fits u64"))
}
