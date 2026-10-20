//! The organization step: each given org made when absent (forge-setup.md, 4.3, step 5).
use super::{
    Event, Need, Reconcile, Reply, Stage, State, Why,
    tokens::{reply, unexpected},
};
use crate::given::Given;
use alloc::boxed::Box;
use skein_forge::forgejo::{document::Document, request::Request};
#[derive(PartialEq, Eq, Debug)]
enum Phase {
    Org,
    Create,
}
#[derive(Debug)]
pub(super) struct Orgs {
    names: Box<[Box<[u8]>]>,
    owner: Box<[u8]>,
    at: usize,
    phase: Phase,
}
impl Orgs {
    /// None when no org is given (a managed forge).
    pub fn new(given: &Given) -> Option<Self> {
        if given.orgs.is_empty() {
            return None;
        }
        let owner = given.org_owner.clone().expect("given::check refuses orgs without an owner");
        Some(Self { names: given.orgs.clone(), owner, at: 0, phase: Phase::Org })
    }
    pub fn name(&self) -> &[u8] {
        self.names.get(self.at).expect("current org")
    }
    fn next(&mut self, machine: &mut Reconcile) -> Need {
        self.at = self.at.checked_add(1).expect("bounded orgs");
        if self.at == self.names.len() {
            machine.start_access()
        } else {
            self.phase = Phase::Org;
            machine.send(Request::Org { name: self.name().into() })
        }
    }
    fn event(&mut self, machine: &mut Reconcile, event: Event) -> Result<Need, Why> {
        let answer = reply(event)?;
        match self.phase {
            Phase::Org | Phase::Create => {
                if let Reply::Document(Document::Org(_)) = answer {
                    Ok(self.next(machine))
                } else if answer == Reply::Status(404) && self.phase == Phase::Org {
                    self.phase = Phase::Create;
                    Ok(machine.send(Request::CreateOrg { person: self.owner.clone(), name: self.name().into() }))
                } else {
                    Err(unexpected(answer))
                }
            }
        }
    }
}
pub(super) fn advance(machine: &mut Reconcile, event: Event) -> Need {
    machine.state = State::Orgs;
    let mut orgs = machine.orgs.take().expect("org phase owns its walk");
    let need = match orgs.event(machine, event) {
        Ok(need) => need,
        Err(why) => machine.stop_named(Stage::Org, orgs.name(), why),
    };
    machine.orgs = Some(orgs);
    need
}
