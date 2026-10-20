//! The owner's plans, answers and retained items (forge-setup.md, 6.3).
use crate::{item::Item, reconcile::Failure};
use skein_lib::{Secret, Token};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plan {
    Reconcile { forge: u16 },
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
    Done { run: Token },
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
