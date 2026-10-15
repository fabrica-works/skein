//! Seeded shell acts on the currently visible tree (skein's ui.md, section 17.1).
//! Only the seed and word bound are kept. Every selection scans bounded tree
//! nodes, and input bytes come from a small alphabet that includes Markdown.
use alloc::boxed::Box;
use alloc::vec::Vec;
use skein_lib::Rng;
use skein_ui::{Act, takes};
use skein_ui::{Node, NodeId, Role, State, Tree};

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz 0123456789\n#*_`[]()>-+.!";

/// One random operation for the world's shell to perform.
#[derive(PartialEq, Eq, Debug)]
pub enum RandomDoing {
    /// Report an act by the currently visible node's id.
    Act { node: NodeId, act: Act },
    /// Go back through this client's address history.
    Back,
    /// Reload this client from its small store.
    Reload,
}

/// A random person whose seed and input bound determine every choice.
#[derive(Debug)]
pub struct RandomPerson {
    rng: Rng,
    word_bytes: u32,
}

impl RandomPerson {
    /// Draw input within the byte limit the world gives for a person's writing.
    #[must_use]
    pub fn new(seed: u64, word_bytes: u32) -> Self {
        Self { rng: Rng::new(seed), word_bytes }
    }

    /// Choose an enabled bound node, or occasionally navigate back or reload.
    /// An empty tree can still yield Back or Reload; otherwise it yields None.
    #[must_use]
    pub fn next(&mut self, tree: &Tree) -> Option<RandomDoing> {
        match self.rng.below(32) {
            0 => return Some(RandomDoing::Back),
            1 => return Some(RandomDoing::Reload),
            _ => {}
        }
        let count = tree.nodes().iter().filter(|node| available(tree, node)).count();
        if count == 0 {
            return None;
        }
        let chosen = usize::try_from(self.rng.below(u64::try_from(count).expect("tree count fits u64")))
            .expect("node index fits usize");
        let node = tree.nodes().iter().filter(|node| available(tree, node)).nth(chosen).expect("chosen node exists");
        let submits = node.states.as_slice().contains(&State::SubmitsOnEnter);
        let act = if takes(node.role, submits, &Act::Toggle) {
            Act::Toggle
        } else if takes(node.role, submits, &Act::Press) {
            Act::Press
        } else if takes(node.role, submits, &Act::Submit) && self.rng.below(2) == 0 {
            Act::Submit
        } else {
            Act::Input { value: self.words() }
        };
        Some(RandomDoing::Act { node: node.id, act })
    }

    fn words(&mut self) -> Box<[u8]> {
        let length = self.rng.below(u64::from(self.word_bytes).checked_add(1).expect("u32 bound plus one fits u64"));
        let mut words = Vec::with_capacity(usize::try_from(length).expect("word length fits usize"));
        for _ in 0..length {
            let index = usize::try_from(self.rng.below(u64::try_from(ALPHABET.len()).expect("alphabet fits u64")))
                .expect("alphabet index fits usize");
            words.push(ALPHABET[index]);
        }
        words.into_boxed_slice()
    }
}

fn available(tree: &Tree, node: &Node) -> bool {
    let submits = node.states.as_slice().contains(&State::SubmitsOnEnter);
    node.role != Role::Drawing
        && node.id != NodeId(0)
        && tree.bound(node.id).is_some()
        && !tree.disabled(node.id)
        && [Act::Toggle, Act::Press, Act::Input { value: Box::new([]) }, Act::Submit]
            .iter()
            .any(|act| takes(node.role, submits, act))
}
