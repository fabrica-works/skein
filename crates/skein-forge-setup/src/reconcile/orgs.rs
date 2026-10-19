//! Organization, team and membership steps, before any hooks (forge-setup.md, 4.3).
use super::{
    Event, Need, Reconcile, Reply, Stage, State, Why,
    tokens::{reply, unexpected},
};
use crate::given::Given;
use alloc::boxed::Box;
use skein_forge::forgejo::{
    document::{Access, Document, Row, TeamSpec, Units},
    request::{Page, Request},
};
#[derive(PartialEq, Eq, Debug)]
enum Phase {
    Settings,
    Org,
    Create,
    Teams,
    Team,
    Members,
    Add,
}
#[derive(Debug)]
pub(super) struct Orgs {
    names: Box<[Box<[u8]>]>,
    owner: Option<Box<[u8]>>,
    at: usize,
    cap: u32,
    page: u32,
    team: u64,
    phase: Phase,
}
impl Orgs {
    pub fn new(given: &Given) -> Self {
        let owner = given.org_owner.clone();
        Self { names: given.orgs.clone(), owner, at: 0, cap: 0, page: 1, team: 0, phase: Phase::Settings }
    }
    pub fn begin(&mut self, cap: Option<u32>) {
        if let Some(cap) = cap {
            self.cap = cap;
            self.phase = Phase::Org;
        }
    }
    pub fn name(&self) -> &[u8] {
        self.names.get(self.at).expect("current org")
    }
    fn listing(&mut self, machine: &Reconcile, members: bool) -> Need {
        let page = Page { page: Some(self.page), limit: Some(self.cap) };
        self.phase = if members { Phase::Members } else { Phase::Teams };
        machine.send(if members {
            Request::TeamMembers { team: self.team, page }
        } else {
            Request::Teams { org: self.name().into(), page }
        })
    }
    fn next(&mut self, machine: &mut Reconcile) -> Need {
        self.at = self.at.checked_add(1).expect("bounded orgs");
        self.page = 1;
        if self.at == self.names.len() {
            machine.start_hooks(self.names.clone(), self.cap)
        } else {
            self.phase = Phase::Org;
            machine.send(Request::Org { name: self.name().into() })
        }
    }
    fn event(&mut self, machine: &mut Reconcile, event: Event) -> Result<Need, Why> {
        let answer = reply(event)?;
        match self.phase {
            Phase::Settings => {
                if let Reply::Document(Document::ApiSettings(settings)) = answer {
                    if settings.max_response_items == 0 {
                        return Err(Why::Unexpected);
                    }
                    self.cap = settings.max_response_items;
                    self.phase = Phase::Org;
                    Ok(machine.send(Request::Org { name: self.name().into() }))
                } else {
                    Err(unexpected(answer))
                }
            }
            Phase::Org | Phase::Create => {
                if let Reply::Document(Document::Org(_)) = answer {
                    Ok(self.listing(machine, false))
                } else if answer == Reply::Status(404) && self.phase == Phase::Org {
                    let person = self.owner.clone().ok_or(Why::OrgMissing)?;
                    self.phase = Phase::Create;
                    Ok(machine.send(Request::CreateOrg { person, name: self.name().into() }))
                } else {
                    Err(unexpected(answer))
                }
            }
            Phase::Teams => self.teams(machine, answer),
            Phase::Team => {
                if let Reply::Document(Document::Team(team)) = answer {
                    self.team = team.id;
                    self.page = 1;
                    Ok(self.listing(machine, true))
                } else {
                    Err(unexpected(answer))
                }
            }
            Phase::Members => {
                let Reply::Rows(rows) = answer else {
                    return Err(unexpected(answer));
                };
                for row in &rows {
                    let Row::User(user) = row else {
                        return Err(Why::Unexpected);
                    };
                    if user.login == machine.bot_login {
                        return Ok(self.next(machine));
                    }
                }
                if rows.len() < usize::try_from(self.cap).expect("u32 fits") {
                    self.phase = Phase::Add;
                    Ok(machine.send(Request::AddMember { team: self.team, login: machine.bot_login.clone() }))
                } else {
                    self.page = self.page.checked_add(1).ok_or(Why::Unexpected)?;
                    Ok(self.listing(machine, true))
                }
            }
            Phase::Add => {
                if answer == Reply::Empty {
                    Ok(self.next(machine))
                } else {
                    Err(unexpected(answer))
                }
            }
        }
    }
    fn teams(&mut self, machine: &Reconcile, answer: Reply) -> Result<Need, Why> {
        let Reply::Rows(rows) = answer else {
            return Err(unexpected(answer));
        };
        let wanted = spec(&machine.bot_login);
        for row in &rows {
            let Row::Team(team) = row else {
                return Err(Why::Unexpected);
            };
            if team.spec.name == machine.bot_login {
                self.team = team.id;
                self.page = 1;
                if team.spec == wanted {
                    return Ok(self.listing(machine, true));
                }
                self.phase = Phase::Team;
                return Ok(machine.send(Request::EditTeam { team: team.id, spec: wanted }));
            }
        }
        if rows.len() < usize::try_from(self.cap).expect("u32 fits") {
            self.phase = Phase::Team;
            Ok(machine.send(Request::CreateTeam { org: self.name().into(), spec: wanted }))
        } else {
            self.page = self.page.checked_add(1).ok_or(Why::Unexpected)?;
            Ok(self.listing(machine, false))
        }
    }
}
fn spec(name: &[u8]) -> TeamSpec {
    TeamSpec {
        name: name.into(),
        permission: Access::Write,
        includes_all_repositories: true,
        units: Units { code: Access::Write, issues: Access::Write, pulls: Access::Write, actions: Access::Write },
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
