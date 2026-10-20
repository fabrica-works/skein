//! Adopted repositories walked in the order given (forge-setup.md, 4.3, step 6).
use super::{Event, Need, Reconcile, Stage, State};
use crate::{
    access::{Access, Fail, Move},
    given::Given,
};
use alloc::boxed::Box;
use skein_forge::forgejo::request::Request;

#[derive(Debug)]
pub(super) struct Walk {
    ids: Box<[u64]>,
    at: usize,
    access: Access,
}
impl Walk {
    pub fn new(given: &Given) -> Option<Self> {
        let id = *given.adopted.first()?;
        Some(Self { ids: given.adopted.clone(), at: 0, access: Access::new(id, &given.bot_login) })
    }
    pub fn begin(&mut self) -> Request {
        self.access.begin()
    }
    fn next(&mut self, machine: &mut Reconcile) -> Need {
        self.at = self.at.checked_add(1).expect("bounded repositories");
        match self.ids.get(self.at) {
            Some(id) => {
                self.access = Access::new(*id, &machine.bot_login);
                let request = self.access.begin();
                machine.send(request)
            }
            None => machine.finish(),
        }
    }
}
pub(super) fn advance(machine: &mut Reconcile, event: Event) -> Need {
    machine.state = State::Access;
    let mut walk = machine.access.take().expect("access phase owns its walk");
    let need = match walk.access.event(event) {
        Ok(Move::Send(request)) => machine.send(request),
        Ok(Move::Done) | Err(Fail::Gone) => walk.next(machine),
        Err(Fail::Why(why)) => machine.stop_named(Stage::Repository, &walk.access.subject(), why),
    };
    machine.access = Some(walk);
    need
}
