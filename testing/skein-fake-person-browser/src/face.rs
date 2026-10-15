//! Queries and acts translated without executing script in the page.
use skein_browser::boundary::{Event, Go, Key, Match, Query, Request, Seen};
use skein_fake_person::{Doing, Face, Find, Found, Target};
use skein_lib::Token;
use skein_ui::{Act, NodeId, Role};
use skein_ui_web::mapping::accessible;

struct Search {
    title: bool,
    find: Find,
    depth: usize,
    role: usize,
    within: Option<u64>,
    op: Token,
}

/// One page's asynchronous semantic face. Nodes are numbered locally.
pub struct BrowserFace<'a> {
    page: Token,
    address: std::borrow::Cow<'a, str>,
    serial: u64,
    requests: Vec<Request>,
    search: Option<Search>,
    answer: Option<Found>,
    seen: Vec<(Seen, Option<Role>)>,
    acting: Option<Token>,
    failure: Option<String>,
    warned: bool,
    go: Option<Token>,
}

impl<'a> BrowserFace<'a> {
    #[must_use]
    pub fn new(page: Token, address: impl Into<std::borrow::Cow<'a, str>>) -> Self {
        Self {
            page,
            address: address.into(),
            serial: page.raw().checked_mul(1_000_000).expect("page operation range fits"),
            requests: Vec::new(),
            search: None,
            answer: None,
            seen: Vec::new(),
            acting: None,
            failure: None,
            warned: false,
            go: None,
        }
    }
    pub fn title(&mut self, words: &[u8]) -> Found {
        if let Some(answer) = self.answer.take() {
            return answer;
        }
        if self.search.is_none() {
            self.ask(Search {
                title: true,
                find: Find::exactly_named(Role::Page, words),
                depth: 0,
                role: 0,
                within: None,
                op: Token::new(0),
            });
        }
        self.answer.take().unwrap_or(Found::Pending)
    }
    #[must_use]
    pub fn warned(&self) -> bool {
        self.warned
    }
    fn token(&mut self) -> Token {
        self.serial = self.serial.checked_add(1).expect("operation numbers fit");
        Token::new(self.serial)
    }
    /// Drain requests into the browser kit before the loop waits.
    pub fn requests(&mut self) -> impl Iterator<Item = Request> + '_ {
        self.requests.drain(..)
    }
    #[must_use]
    pub fn pending(&self) -> bool {
        self.search.is_some() || self.acting.is_some()
    }
    pub(crate) fn fail(&mut self, why: String) {
        self.failure = Some(why);
    }
    pub fn failure(&mut self) -> Option<String> {
        self.failure.take()
    }
    fn ask(&mut self, mut search: Search) {
        let target = search.find.within.get(search.depth).unwrap_or(&search.find.target);
        let (mut roles, name, matching) = query_parts(target);
        if search.title {
            roles = &[b"RootWebArea"];
        }
        let Some(role) = roles.get(search.role) else {
            self.answer = Some(Found::Absent);
            return;
        };
        search.op = self.token();
        self.requests.push(Request::Find {
            page: self.page,
            op: search.op,
            query: Query { role: Box::from(*role), name: name.into(), matching, within: search.within, boxes: false },
        });
        self.search = Some(search);
    }
    fn remember(&mut self, seen: &Seen, role: Option<Role>) -> NodeId {
        if let Some(at) = self.seen.iter().position(|(old, _)| old.node == seen.node) {
            self.seen[at] = (seen.clone(), role);
            return NodeId(u32::try_from(at).expect("node numbers fit"));
        }
        let id = NodeId(u32::try_from(self.seen.len()).expect("node numbers fit"));
        self.seen.push((seen.clone(), role));
        id
    }
    /// Deliver the kit's terminal events. Other pages' operations are ignored.
    pub fn event(&mut self, event: &Event) {
        match event {
            Event::Warned { op } => {
                if self.go == Some(*op) && self.acting == Some(*op) {
                    self.warned = true;
                }
            }
            Event::Found { op, seen, .. } => {
                if self.search.as_ref().map(|s| s.op) != Some(*op) {
                    return;
                }
                let mut search = self.search.take().expect("matching search");
                if let Some(seen) = seen.get(0) {
                    if search.depth < search.find.within.len() {
                        search.depth += 1;
                        search.role = 0;
                        search.within = Some(seen.node);
                        self.ask(search);
                    } else {
                        let role = target_role(&search.find.target);
                        self.answer = Some(Found::Node(self.remember(seen, role)));
                    }
                } else {
                    search.role += 1;
                    self.ask(search);
                }
            }
            Event::Done { op } => {
                if self.acting == Some(*op) {
                    self.acting = None;
                }
            }
            Event::Refused { op, why } => {
                if self.search.as_ref().map(|s| s.op) == Some(*op) {
                    self.search = None;
                    self.answer = Some(Found::Absent);
                }
                if self.acting == Some(*op) {
                    self.acting = None;
                    self.failure = Some(format!("browser refused act: {why:?}"));
                }
            }
            Event::Ready { .. }
            | Event::Opened { .. }
            | Event::Met { .. }
            | Event::Missed { .. }
            | Event::Snapshot { .. }
            | Event::Screenshot { .. }
            | Event::Trouble { .. }
            | Event::Closed { .. } => {}
        }
    }
    /// Translate one person's operation, naming a field when replacement is unavailable.
    ///
    /// # Errors
    /// An unknown node, drawing act or nonempty field cannot be acted on.
    pub fn perform(&mut self, doing: Doing<Box<[u8]>>) -> Result<(), String> {
        let op = self.token();
        let request = match doing {
            Doing::Go { address } => {
                self.warned = false;
                self.go = Some(op);
                Request::Go {
                    page: self.page,
                    op,
                    to: Go::Address(
                        format!("http://{}{}", self.address, String::from_utf8_lossy(&address)).into_bytes().into(),
                    ),
                }
            }
            Doing::Reload => {
                self.warned = false;
                self.go = Some(op);
                Request::Go { page: self.page, op, to: Go::Reload }
            }
            Doing::Act { node, act } => {
                let (seen, _) = self
                    .seen
                    .get(usize::try_from(node.0).expect("node fits"))
                    .ok_or_else(|| format!("unknown node {}", node.0))?;
                match act {
                    Act::Press | Act::Toggle => Request::Press { page: self.page, op, node: seen.node, at: None },
                    Act::Input { value } => {
                        if !seen.value.is_empty() {
                            return Err(format!(
                                "field '{}' holds words; the kit cannot replace them",
                                String::from_utf8_lossy(&seen.name)
                            ));
                        }
                        Request::Type { page: self.page, op, node: seen.node, text: value }
                    }
                    Act::Submit => Request::Key { page: self.page, op, key: Key::Enter },
                    Act::Drawing { .. } => return Err("a person cannot press a drawing mark".into()),
                }
            }
        };
        self.acting = Some(op);
        self.requests.push(request);
        Ok(())
    }
}
impl Face for BrowserFace<'_> {
    fn find(&mut self, find: &Find) -> Found {
        if let Some(answer) = self.answer.take() {
            return answer;
        }
        if self.search.is_none() {
            self.ask(Search { title: false, find: find.clone(), depth: 0, role: 0, within: None, op: Token::new(0) });
        }
        self.answer.take().unwrap_or(Found::Pending)
    }
    fn allows(&self, node: NodeId, _act: &Act) -> bool {
        self.seen.get(usize::try_from(node.0).expect("node fits")).is_some_and(|(seen, _)| !seen.states.disabled)
    }
    fn role(&self, node: NodeId) -> Option<Role> {
        self.seen.get(usize::try_from(node.0).expect("node fits")).and_then(|(_, role)| *role)
    }
}
fn target_role(target: &Target) -> Option<Role> {
    match target {
        Target::Role { role, .. } | Target::ExactRole { role, .. } => Some(*role),
        Target::Text { .. } => None,
    }
}
fn query_parts(target: &Target) -> (&'static [&'static [u8]], &[u8], Match) {
    match target {
        Target::Role { role, name } => (accessible(*role), name, Match::Words),
        Target::ExactRole { role, name } => (accessible(*role), name, Match::Exact),
        Target::Text { text } => (&[b"StaticText"], text, Match::Words),
    }
}
