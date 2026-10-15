//! Accessibility roles and owned bytes, with no layout or application types.
//! Nodes are preorder; a subtree occupies `size` consecutive nodes including
//! its root (skein's ui.md, section 3). New builds have unassigned ids;
//! the diff assigns ids when the tree becomes visible.

use alloc::boxed::Box;
use skein_lib::{List, Token};

/// A visible node's identity, preserved while it matches across builds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(pub u32);

/// An opaque owner of tokens, keys, styles, regions, pictures and phrases.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Owner {
    pub number: u32,
}

/// A node's meaning, mapped exhaustively by every shell.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Role {
    /// A page node.
    Page,
    /// The banner landmark.
    Banner,
    /// The navigation landmark.
    Navigation,
    /// The main landmark.
    Main,
    /// The complementary landmark.
    Complementary,
    /// The content information landmark.
    ContentInfo,
    /// The form landmark.
    Form,
    /// The search landmark.
    Search,

    /// A region node.
    Region,
    /// A heading at a semantic level.
    Heading { level: u8 },
    /// A paragraph node.
    Paragraph,
    /// A text node.
    Text,
    /// A list node.
    List,
    /// A item node.
    Item,
    /// A table node.
    Table,
    /// A row node.
    Row,
    /// A cell node.
    Cell,
    /// A term node.
    Term,
    /// A description node.
    Description,
    /// A button node.
    Button,
    /// A link node.
    Link,
    /// A textfield node.
    TextField,
    /// A textarea node.
    TextArea,
    /// A choice node.
    Choice,
    /// A choicegroup node.
    ChoiceGroup,
    /// A check node.
    Check,
    /// A dialog node.
    Dialog,
    /// A status node.
    Status,
    /// A alert node.
    Alert,
    /// A log node.
    Log,
    /// A tree node.
    Tree,
    /// A treeitem node.
    TreeItem,
    /// A tabs node.
    Tabs,
    /// A tab node.
    Tab,
    /// A progress node.
    Progress,
    /// A time node.
    Time,
    /// A code node.
    Code,
    /// A quote node.
    Quote,
    /// A separator node.
    Separator,
    /// A named group of additional acts.
    Menu,
    /// A bound act inside a menu.
    MenuItem,
    /// A region a shell may fold.
    Disclosure,
    /// A named picture from its owner.
    Image,

    /// A figure node.
    Figure,
    /// A slot node.
    Slot,
    /// A drawing node.
    Drawing,
}

/// A semantic state, never a visual style.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum State {
    /// The node is disabled.
    Disabled,
    /// The node is expanded.
    Expanded,
    /// The node is busy.
    Busy,
    /// The node is current.
    Current,
    /// The node is invalid.
    Invalid,
    /// The node is pressed.
    Pressed,
    /// The node is selected.
    Selected,
    /// The node is submitsonenter.
    SubmitsOnEnter,
}

/// A token from an owner's closed style set.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Style {
    pub owner: Owner,
    pub token: u16,
}

/// A region's language-free name from its owner's closed set (ui.md, 8).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Region {
    pub owner: Owner,
    pub number: u16,
}
/// One of an owner's pictures, supplied when a shell is built (ui.md, 3.5).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Picture {
    pub owner: Owner,
    pub number: u16,
}
/// A field's keyboard or control (ui.md, 3.4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EntryKind {
    Text,
    Number,
    Email,
    Telephone,
    Url,
    Date,
    Time,
    Secret,
}
/// The rows and columns a cell spans, each at least one (ui.md, 3.2).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Spans {
    pub rows: u16,
    pub columns: u16,
}
/// An image's owner picture or checked address (ui.md, 3.5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Source {
    Picture(Picture),
    Address(Box<[u8]>),
}
/// A key a bound node answers to as a press (ui.md, 6.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Shortcut {
    pub key: u8,
    pub modifier: Modifier,
}
impl Shortcut {
    /// Escape is named only inside a dialog (ui.md, 6.1 and 14).
    pub const ESCAPE: Shortcut = Shortcut { key: 0x1b, modifier: Modifier::Plain };
}
/// The modifier pressed with a shortcut's key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Modifier {
    Plain,
    Primary,
    Alt,
}

/// The meaning of a span of text.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Run {
    /// Ordinary words.
    Plain,
    /// Emphasized words.
    Emphasis,
    /// Strongly emphasized words.
    Strong,
    /// Literal code.
    Code,
    /// A target interpreted by the text's owner.
    Link { to: Token },
}

/// One owned span on a text node.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TextRun {
    pub run: Run,
    pub text: Box<[u8]>,
}

/// A field's value and count of the owner's own writes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Value {
    pub text: Box<[u8]>,
    pub written: u64,
}

/// The object an item represents, independent of its sibling position.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub owner: Owner,
    pub kind: u16,
    pub a: u64,
    pub b: u64,
}

/// A fragment requested from its owner's view.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SlotRef {
    pub owner: Owner,
    pub token: Token,
}

/// An owner's drawing vocabulary number.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DrawingKind {
    pub owner: Owner,
    pub kind: u16,
}

/// Opaque drawing data; ordinary child nodes provide its readable fallback.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Drawing {
    pub kind: DrawingKind,
    pub data: Box<[u8]>,
}

/// One node and its attributes; `size` includes its descendants.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Node {
    pub id: NodeId,
    pub role: Role,
    pub size: u32,
    pub name: Option<Box<[u8]>>,
    pub states: List<State>,
    pub styles: List<Style>,
    pub region: Option<Region>,
    pub shortcut: Option<Shortcut>,
    pub source: Option<Source>,
    pub description: Option<Box<[u8]>>,
    pub entry: Option<EntryKind>,
    pub focus: Option<u64>,
    pub count: Option<u32>,
    pub position: Option<u32>,
    pub spans: Option<Spans>,
    pub text: List<TextRun>,
    pub value: Option<Value>,
    pub link: Option<crate::Binding>,
    /// A checked external Markdown target, independent of application addresses.
    pub external: Option<Box<[u8]>>,
    pub key: Option<Key>,
    pub binding: Option<crate::Binding>,
    pub slot: Option<SlotRef>,
    pub drawing: Option<Drawing>,
}

impl Node {
    pub(crate) fn vacant() -> Node {
        Node {
            id: NodeId(0),
            role: Role::Text,
            size: 1,
            name: None,
            states: List::with_capacity(0),
            styles: List::with_capacity(0),
            region: None,
            shortcut: None,
            source: None,
            description: None,
            entry: None,
            focus: None,
            count: None,
            position: None,
            spans: None,
            text: List::with_capacity(0),
            value: None,
            link: None,
            external: None,
            key: None,
            binding: None,
            slot: None,
            drawing: None,
        }
    }

    pub(crate) fn new(role: Role, limits: &crate::Limits, runs: u32) -> Node {
        Node {
            id: NodeId(0),
            role,
            size: 1,
            name: None,
            states: List::with_capacity(9),
            styles: List::with_capacity(limits.styles_per_node),
            region: None,
            shortcut: None,
            source: None,
            description: None,
            entry: None,
            focus: None,
            count: None,
            position: None,
            spans: None,
            text: List::with_capacity(runs),
            value: None,
            link: None,
            external: None,
            key: None,
            binding: None,
            slot: None,
            drawing: None,
        }
    }

    pub(crate) fn text_bytes(&self) -> u32 {
        let mut bytes = 0_u32;
        if let Some(name) = &self.name {
            bytes = length(name);
        }
        if let Some(description) = &self.description {
            bytes = bytes.checked_add(length(description)).expect("node bytes fit u32");
        }
        if let Some(source) = &self.source {
            match source {
                Source::Picture(_) => {}
                Source::Address(address) => bytes = bytes.checked_add(length(address)).expect("node bytes fit u32"),
            }
        }
        if let Some(value) = &self.value {
            bytes = bytes.checked_add(length(&value.text)).expect("node bytes fit u32");
        }
        if let Some(external) = &self.external {
            bytes = bytes.checked_add(length(external)).expect("node bytes fit u32");
        }
        for run in &self.text {
            bytes = bytes.checked_add(length(&run.text)).expect("node bytes fit u32");
        }
        bytes
    }
}

pub(crate) fn length(bytes: &[u8]) -> u32 {
    u32::try_from(bytes.len()).expect("view bytes fit u32")
}

/// A single tree in preorder, sized before building.
#[derive(Debug)]
pub struct Tree {
    pub(crate) nodes: List<Node>,
}

impl Tree {
    /// Reserve node storage once.
    #[must_use]
    pub fn new(capacity: u32) -> Tree {
        Tree { nodes: List::with_capacity(capacity) }
    }
    /// The tree's preorder nodes.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        self.nodes.as_slice()
    }
    /// A node by its preorder index.
    #[must_use]
    pub fn get(&self, index: u32) -> Option<&Node> {
        self.nodes.get(index)
    }
    /// A node by its current visible identity.
    #[must_use]
    #[expect(clippy::manual_find, reason = "step code uses bounded scans without closures")]
    pub fn find(&self, id: NodeId) -> Option<&Node> {
        for node in &self.nodes {
            if node.id == id {
                return Some(node);
            }
        }
        None
    }
    /// Resolve an act's token in this displayed tree. A removed or unbound node
    /// gives nothing. A link's address binding also names its navigation act.
    #[must_use]
    pub fn bound(&self, id: NodeId) -> Option<crate::Binding> {
        if id == NodeId(0) {
            return None;
        }
        let node = self.find(id)?;
        node.binding.or(node.link)
    }

    /// Resolve the whole shell act; the owner interprets its token and value.
    #[must_use]
    pub fn act(&self, id: NodeId, act: crate::Act) -> Option<crate::BoundAct> {
        let binding = self.bound(id)?;
        Some(crate::BoundAct { binding, act })
    }

    /// Ask the view root to translate this link owner's address for the shell.
    #[must_use]
    pub fn address(&self, id: NodeId) -> Option<crate::Binding> {
        if id == NodeId(0) {
            return None;
        }
        self.find(id)?.link
    }

    /// A checked external Markdown link for a shell to show directly.
    #[must_use]
    pub fn external(&self, id: NodeId) -> Option<&[u8]> {
        if id == NodeId(0) {
            return None;
        }
        self.find(id)?.external.as_deref()
    }

    /// The number of nodes, including every fallback child.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.nodes.len()
    }
    /// Whether no nodes were built.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
