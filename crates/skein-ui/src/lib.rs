//! Portable UI trees, building and diffs (ui.md, sections 3, 5 and 7).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;
mod binding;
pub(crate) mod builder;
pub(crate) mod diff;
mod limits;
pub mod look;
pub mod markdown;
pub mod outline;
pub mod phrases;
pub mod scene;
pub(crate) mod tree;
mod view;

pub use binding::{Act, Binding, BoundAct, number, takes};
pub use builder::{Builder, Slot, SlotId};
pub use diff::{Attribute, Made, Patch, diff};
pub use limits::{Limits, worst_case};
pub use tree::{
    Drawing, DrawingKind, EntryKind, Key, Modifier, Node, NodeId, Owner, Picture, Region, Role, Run, Shortcut, SlotRef,
    Sort, Source, Spans, State, Style, TextRun, Tree, Value,
};
pub use view::View;

#[cfg(test)]
pub(crate) mod tests;
