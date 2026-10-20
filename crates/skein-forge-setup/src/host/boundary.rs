//! The owner's plans, answers and retained items (forge-setup.md, 6.3).
use crate::{
    item::Item,
    reconcile::{Failure, Outcome},
};
use alloc::boxed::Box;
use core::num::{NonZeroU32, NonZeroU64};
use skein_lib::{Secret, Token};
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Plan {
    Reconcile {
        forge: u16,
        adopted: Box<[u64]>,
    },
    /// A connected Forgejo's two tokens, checked (4.6).
    Check {
        forge: u16,
    },
    /// The handed admin token, held only until the run ends (4.7).
    TakeOver {
        forge: u16,
        admin_token: Secret,
    },
    /// The bot's access to the repository whose id the forge gave (4.5).
    Adopt {
        forge: u16,
        repository: NonZeroU64,
    },
    /// One picker page, from 1 (4.8). Forgejo takes no installation.
    /// Reads only: the host forwards no Write for it.
    Reachable {
        forge: u16,
        installation: Option<u64>,
        page: NonZeroU32,
    },
}
impl Plan {
    pub(super) fn forge(&self) -> u16 {
        match self {
            Self::Reconcile { forge, .. }
            | Self::Check { forge }
            | Self::TakeOver { forge, .. }
            | Self::Adopt { forge, .. }
            | Self::Reachable { forge, .. } => *forge,
        }
    }
    /// Whether the plan may change the forge or the owner's items.
    pub(super) fn writes(&self) -> bool {
        match self {
            Self::Reconcile { .. } | Self::Check { .. } | Self::TakeOver { .. } | Self::Adopt { .. } => true,
            Self::Reachable { .. } => false,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Request {
    Run { run: Token, plan: Plan },
    Answer { run: Token, answer: Answer },
    Close,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Answer {
    Read(Option<Secret>),
    Written,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Need {
    Read(Item),
    Write { item: Item, bytes: Secret },
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Event {
    Need { run: Token, need: Need },
    Done { run: Token, outcome: Outcome },
    Stop { run: Token, failure: Failure },
    Closed,
}
/// Output reservations for each entry point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MaxOut {
    pub above: u32,
    pub below: u32,
}
/// Reserve these slots before an entry point runs.
pub const MAX_OUT: MaxOut = MaxOut { above: 4, below: 4 };
