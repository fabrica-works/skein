//! Patch p0002: audio books become ebooks (codec.md, 11.5).

use super::Context;
use super::kept::shelf::{v2, v3};
use skein_codec::patch::{Build, Halt, Old, Site};
use skein_codec::value::Node;

/// Site 0, `written Format`, shelf 2 to 3: carry the surviving variants.
pub fn format(old: &Old, new: &mut Build, at: Node, _cx: &mut Context) -> Result<(), Halt> {
    if old.tag(at)? == v2::format::AUDIO {
        return new.put_variant(v3::format::EBOOK);
    }
    new.carry(old, at)
}

/// The module's site number follows it when the command moves p0002.
const _: Site = Site::new(2, 0);
