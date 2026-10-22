//! The browser's vocabulary above and below its step machine.

use alloc::boxed::Box;

use skein_lib::stream;
use skein_lib::{Duration, List, Token};

/// A backend node id from Chromium's accessibility tree.
pub type Node = u64;

/// An event from one of the child's pipes.
#[derive(PartialEq, Eq, Debug)]
pub enum Below {
    Commands(stream::Up),
    Replies(stream::Up),
    Errors(stream::Up),
}

/// A request to one of the child's pipes.
#[derive(PartialEq, Eq, Debug)]
pub enum Down {
    Commands(stream::Down),
    Replies(stream::Down),
    Errors(stream::Down),
}

/// What the test asks a browser to do. `Address` reads the page's address, its history's current
/// entry; its terminal is `Event::Address` or `Refused`.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    Person { person: Token },
    Page { person: Token, page: Token, url: Box<[u8]> },
    Go { page: Token, op: Token, to: Go },
    Find { page: Token, op: Token, query: Query },
    Await { page: Token, op: Token, query: Query, expect: Expect, within: Duration },
    Press { page: Token, op: Token, node: Node, at: Option<Point> },
    Type { page: Token, op: Token, node: Node, text: Box<[u8]> },
    Compose { page: Token, op: Token, node: Node, text: Box<[u8]> },
    Keys { page: Token, op: Token, node: Node, text: Box<[u8]> },
    Key { page: Token, op: Token, key: Key, shift: bool },
    Address { page: Token, op: Token },
    Attribute { page: Token, op: Token, selector: Box<[u8]>, name: Box<[u8]> },
    Snapshot { page: Token, op: Token },
    Screenshot { page: Token, op: Token },
    /// The lab's look reads alone (browser.md, 5.1); no test sends one.
    Look { page: Token, op: Token, look: Look },
    Close { entity: Token },
}

/// What the browser reports to the test.
#[derive(Debug)]
pub enum Event {
    Ready {
        version: Box<[u8]>,
    },
    Opened {
        page: Token,
    },
    Found {
        op: Token,
        seen: List<Seen>,
        more: u32,
    },
    Met {
        op: Token,
        seen: List<Seen>,
    },
    Missed {
        op: Token,
        seen: List<Seen>,
        more: u32,
    },
    Done {
        op: Token,
    },
    /// The go accepted its page's closing warning; it remains in flight.
    Warned {
        op: Token,
    },
    Refused {
        op: Token,
        why: Refusal,
    },
    /// The URL an `Address` request read, whole, at most `Limits::text` bytes; terminal.
    Address {
        op: Token,
        url: Box<[u8]>,
    },
    Attribute {
        op: Token,
        read: Read,
    },
    Snapshot {
        op: Token,
        text: Box<[u8]>,
    },
    Screenshot {
        op: Token,
        png: Box<[u8]>,
    },
    /// A look's answer; terminal.
    Looked {
        op: Token,
        looked: Looked,
    },
    Trouble {
        page: Token,
        trouble: Trouble,
        text: Box<[u8]>,
    },
    Closed {
        owner: Token,
    },
}

/// What a page looks like to a person's eyes, for the lab's browser alone (browser.md, 5.1).
#[derive(PartialEq, Eq, Debug)]
pub enum Look {
    /// The page's metrics at scale 1 and its media, held until the next or the page's close.
    Viewport { width: u32, height: u32, mobile: bool, scheme: Scheme, motion: Motion },
    /// The accessibility tree under the document, as entries in document order.
    Tree,
    /// The first element a CSS selector finds in the current document.
    Select { selector: Box<[u8]> },
    /// A node's computed values for the properties named, the faces that drew its text, its border box.
    Style { node: Node, properties: List<Box<[u8]>> },
    /// A PNG of the clip in the document's CSS pixels, none for the viewport, drawn at scale 1 or 2.
    Shot { clip: Option<Rect>, scale: u8 },
}

/// The colour scheme a page is drawn for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scheme {
    Light,
    Dark,
}

/// The motion a page is drawn for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Motion {
    Reduce,
    NoPreference,
}

/// A look's answer.
#[derive(PartialEq, Eq, Debug)]
pub enum Looked {
    Viewport,
    Tree(List<Entry>),
    Selected(Option<Node>),
    Style { values: List<Property>, faces: List<Face>, rect: Option<Rect> },
    /// A PNG.
    Shot(Box<[u8]>),
}

/// One node of the accessibility tree, ignored nodes left out and their children lifted.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    /// None for an accessibility node without a backend node.
    pub node: Option<Node>,
    pub depth: u32,
    pub role: Box<[u8]>,
    /// Cut at `Limits::text`.
    pub name: Box<[u8]>,
    /// A heading's or a tree item's level; 0 for none.
    pub level: u32,
}

/// A computed value, whole, as the browser computes it (`16px`, `rgb(17, 17, 17)`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Property {
    pub name: Box<[u8]>,
    pub value: Box<[u8]>,
}

/// A face that drew a node's own text.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Face {
    pub family: Box<[u8]>,
    /// A web font.
    pub custom: bool,
    pub glyphs: u32,
}

/// A selected element's whole markup attribute, including an empty value.
#[derive(PartialEq, Eq, Debug)]
pub enum Read {
    NoElement,
    NoAttribute,
    Value(Box<[u8]>),
}

/// Where to navigate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Go {
    Address(Box<[u8]>),
    Reload,
    Back,
    Forward,
}

/// A key action, independent of a keyboard layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Enter,
    Escape,
    Tab,
    Backspace,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

/// An accessible role and name, optionally scoped beneath a node.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Query {
    pub role: Box<[u8]>,
    pub name: Box<[u8]>,
    pub matching: Match,
    pub within: Option<Node>,
    pub boxes: bool,
}

/// How an accessible name matches the query.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Match {
    Exact,
    Words,
}

/// Whole CSS pixels from the node's box's top left.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

/// What an expectation asks of the query's result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Expect {
    Present,
    Count(u32),
    Absent,
}

/// An accessible node that matched a query.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Seen {
    pub node: Node,
    pub role: Box<[u8]>,
    pub name: Box<[u8]>,
    pub value: Box<[u8]>,
    pub states: States,
    pub rect: Option<Rect>,
}

/// The states exposed in an accessibility node.
#[expect(clippy::struct_excessive_bools, reason = "each state is an independent accessibility property")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct States {
    pub focused: bool,
    pub disabled: bool,
    pub checked: bool,
    pub expanded: bool,
    pub selected: bool,
}

/// A box in whole CSS pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub width: u64,
    pub height: u64,
}

/// Why an action could not be performed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refusal {
    Gone,
    Hidden,
    Covered,
    Disabled,
    Crashed,
    Limit,
    Timeout,
    Protocol,
}

/// An error observed in the page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trouble {
    Exception,
    Console,
    Log,
    Crash,
    Dialog,
}
