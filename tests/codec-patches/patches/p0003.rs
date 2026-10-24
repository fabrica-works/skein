//! Patch p0003: audio books become ebooks (codec.md, 11.5).

use super::Context;
use super::kept::shelf::p0003::{new, old};
use skein_codec::patch::{Build, Halt, Old, Site};
use skein_codec::value::Node;

/// Site 0, `written Format`, shelf 2 to 3: carry the surviving variants.
pub fn format(old: &Old, new: &mut Build, at: Node, _cx: &mut Context) -> Result<(), Halt> {
    if old.tag(at)? == old::format::AUDIO {
        return new.put_variant(new::format::EBOOK);
    }
    new.carry(old, at)
}

/// The module's site number follows it when the command moves p0003.
const _: Site = Site::new(3, 0);
