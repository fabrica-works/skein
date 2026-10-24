//! Patch 0002: the command wrote the skeleton; its functions are the author's (codec.md, 11.5).

use super::Context;
use skein_codec::patch::{Build, Halt, Old, Site};
use skein_codec::value::Node;

/// Site 0, `written Format`, shelf 2 to 3.
///
/// - `drop Format.audio`
pub fn format(_old: &Old, _new: &mut Build, _at: Node, _cx: &mut Context) -> Result<(), Halt> {
    Err(Halt::NoSite(Site::new(2, 0)))
}
