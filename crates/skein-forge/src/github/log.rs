//! The App's delivery log values (forge-github.md, 9.3).
use crate::api;
use alloc::boxed::Box;
use skein_lib::Wall;

// The header parser and the log share one checked cursor representation.
pub use super::header::Cursor;

/// One delivery, in the log's newest-first order. Ids retain every u64 digit.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Row {
    pub id: u64,
    pub guid: [u8; 16],
    pub event: Box<[u8]>,
    pub action: Option<Box<[u8]>>,
    pub installation_id: Option<u64>,
    pub repository_id: Option<u64>,
    pub status_code: u16,
    pub delivered_at: Wall,
    pub redelivery: bool,
}

/// Which work a log row asks the puller to consider.
/// `Repositories` carries no installation: the log row has none (9.3),
/// so its installation comes from reading the delivery whole.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Installations,
    Repositories,
    Hinted,
    Other,
}
impl Row {
    #[must_use]
    pub fn kind(&self) -> Kind {
        use super::delivery::{Event, event};
        match event(&self.event) {
            Some(Event::Installation) => Kind::Installations,
            Some(Event::Repositories) => Kind::Repositories,
            Some(
                Event::Issue
                | Event::Comment
                | Event::Pull
                | Event::Review
                | Event::Remark
                | Event::Push
                | Event::Ref(_)
                | Event::Ci(_),
            ) => Kind::Hinted,
            Some(Event::Target) | None => Kind::Other,
        }
    }
}

/// A trusted logged payload and its checked X-GitHub-Event word.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Logged {
    pub event: Box<[u8]>,
    pub delivery: super::delivery::Delivery,
}

/// Only the first page carries an entity tag.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Ask {
    First { validator: Option<api::Validator> },
    Next { cursor: Cursor },
}

/// A conditional hit is distinct from an empty page and costs nothing.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Listed {
    Unchanged,
    Page(Page),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Page {
    pub rows: Box<[Row]>,
    pub next: Option<Cursor>,
    pub validator: Option<api::Validator>,
}
