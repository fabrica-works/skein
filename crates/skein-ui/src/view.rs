//! The last displayed tree and a spare (skein's ui.md, section 7).
//! `stale` compares the caller's shown count. `builder` lends the spare as owned
//! construction state; the caller builds and splices slots, then `finish`
//! emits patches and keeps the new tree. This module knows no owner objects.
//!
//! State | Event | Next state | Emits
//! Unbuilt | finish | Built at shown | Initial insert
//! Built | unchanged shown | Built | Nothing
//! Built | finish at changed shown | Built at shown | Diff patches

use crate::Limits;
use crate::{Act, Binding, BoundAct};
use crate::{Builder, NodeId, Patch, Tree, diff};

use skein_lib::Queue;

/// A visible tree, its spare allocation, shown count and never-reused node ids.
#[derive(Debug)]
pub struct View {
    last: Option<Tree>,
    spare: Option<Tree>,
    built: Option<u64>,
    next_id: u32,
    limits: Limits,
}

impl View {
    /// Reserve both trees before any build.
    #[must_use]
    pub fn new(limits: &Limits) -> View {
        View {
            last: Some(Tree::new(limits.nodes)),
            spare: Some(Tree::new(limits.nodes)),
            built: None,
            next_id: 0,
            limits: *limits,
        }
    }
    /// Whether the owner's count of what is shown differs from the last build.
    #[must_use]
    pub fn stale(&self, shown: u64) -> bool {
        self.built != Some(shown)
    }
    /// Take the spare's room; it must be returned with `finish` before another build.
    #[must_use]
    pub fn builder(&mut self) -> Builder {
        let tree = self.spare.take().expect("one build at a time");
        Builder::reuse(tree, &self.limits)
    }
    /// Finish a complete, already-spliced tree, emit its diff and keep it.
    /// Building for an unchanged count returns the spare without emitting.
    pub fn finish(&mut self, built: Builder, shown: u64, out: &mut Queue<Patch>) {
        assert!(self.spare.is_none(), "finish returns the outstanding builder");
        assert!(built.limits == self.limits, "builder uses the view limits");
        let mut new = built.finish();
        if !self.stale(shown) {
            self.spare = Some(new);
            return;
        }
        let old = self.last.as_ref().expect("view has its last tree");
        diff(old, &mut new, &self.limits, &mut self.next_id, out);
        let previous = self.last.replace(new).expect("view has its last tree");
        self.spare = Some(previous);
        self.built = Some(shown);
    }
    /// The tree whose ids the shell currently sees.
    #[must_use]
    pub fn tree(&self) -> &Tree {
        self.last.as_ref().expect("view has its last tree")
    }
    /// Resolve a node in the last displayed tree to its owner's binding.
    #[must_use]
    pub fn bound(&self, node: NodeId) -> Option<Binding> {
        self.tree().bound(node)
    }
    /// Resolve a shell act in the last displayed tree.
    #[must_use]
    pub fn act(&self, node: NodeId, act: Act) -> Option<BoundAct> {
        match &act {
            Act::Drawing { data } => {
                if data.len() > usize::try_from(self.limits.drawing_bytes).expect("drawing limit fits usize") {
                    return None;
                }
            }
            Act::Press | Act::Input { .. } | Act::Submit | Act::Toggle => {}
        }
        self.tree().act(node, act)
    }
    /// A checked external target the shell may show without an owner translation.
    #[must_use]
    pub fn external(&self, node: NodeId) -> Option<&[u8]> {
        self.tree().external(node)
    }
    /// Hand the link's address binding to the view root for translation.
    #[must_use]
    pub fn address(&self, node: NodeId) -> Option<Binding> {
        self.tree().address(node)
    }
}
