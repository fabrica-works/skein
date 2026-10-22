//! Configured UI storage (skein's ui.md, section 15).
//! The caller derives these capacities from the domain and its own fragments.
//! `worst_case` counts owned heap, excluding allocator overhead, for the visible
//! tree, builders and their splice scratch, diff scratch, output patches and a scene.

use crate::builder::PlacedSlot;
use crate::diff::Open;
use crate::{Made, Node, Patch, Slot, State, Style, TextRun};
use skein_lib::{List, Queue, Stack};

/// Capacities checked before each builder allocation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    pub nodes: u32,
    pub depth: u32,
    /// Name, description, placeholder, runs, field value and external target together on a node.
    pub text_per_node: u32,
    /// Name, description, placeholder, runs, field value and external target together across the tree.
    pub text_bytes: u32,
    pub drawing_bytes: u32,
    pub styles_per_node: u32,
    pub patches: u32,
    pub bindings: u32,
    pub slots: u32,
    pub markdown_depth: u32,
    pub scene_marks: u32,
    pub scene_bytes: u32,
}

/// Heap for rendering, splicing, diff scratch, patch payloads and one scene.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let nodes = List::<Node>::worst_case(limits.nodes)?;
    let states = List::<State>::worst_case(10)?.checked_mul(u64::from(limits.nodes))?;
    let styles = List::<Style>::worst_case(limits.styles_per_node)?.checked_mul(u64::from(limits.nodes))?;
    let runs = List::<TextRun>::worst_case(limits.text_per_node)?.checked_mul(u64::from(limits.nodes))?;
    let drawings = u64::from(limits.drawing_bytes).checked_mul(u64::from(limits.nodes))?;
    let payloads = u64::from(limits.text_bytes).checked_add(drawings)?;
    let tree = nodes.checked_add(states)?.checked_add(styles)?.checked_add(runs)?.checked_add(payloads)?;
    let trees = tree.checked_mul(5)?.checked_add(nodes.checked_mul(2)?)?;
    let stacks = Stack::<u32>::worst_case(limits.depth)?.checked_mul(3)?;
    let slots = List::<PlacedSlot>::worst_case(limits.slots)?
        .checked_mul(3)?
        .checked_add(List::<Slot>::worst_case(limits.slots)?.checked_mul(2)?)?;
    let patches = Queue::<Patch>::worst_case(limits.patches)?;
    let made = List::<Made>::worst_case(limits.nodes)?;
    let maps = List::<Option<u32>>::worst_case(limits.nodes)?.checked_mul(4)?;
    let children = List::<u32>::worst_case(limits.nodes)?;
    let ancestry = Stack::<Open>::worst_case(limits.depth)?.checked_mul(2)?;
    let markdown = crate::markdown::worst_case(limits.text_bytes, limits.markdown_depth)?;
    trees
        .checked_add(crate::scene::worst_case(limits)?)?
        .checked_add(markdown)?
        .checked_add(stacks)?
        .checked_add(slots)?
        .checked_add(patches)?
        .checked_add(made)?
        .checked_add(maps)?
        .checked_add(children)?
        .checked_add(ancestry)
}
