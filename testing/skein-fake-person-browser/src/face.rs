//! Queries and acts translated without executing script in the page.
use skein_browser::boundary::{Event, Go, Key, Match, Query, Read, Request, Seen};
use skein_fake_person::{Doing, Face, Find, Found, Target};
use skein_lib::Token;
use skein_ui::{Act, NodeId, Role};
use skein_ui_web::mapping::accessible;

struct Choice {
    option: u64,
    selects: std::collections::VecDeque<u64>,
    select: Option<u64>,
    keys: std::collections::VecDeque<Key>,
}

struct Search {
    title: bool,
    boxes: bool,
    find: Find,
    depth: usize,
    role: usize,
    within: Option<u64>,
    op: Token,
}

struct Keeping {
    op: Token,
    selector: Box<[u8]>,
    attribute: Box<[u8]>,
    name: Box<[u8]>,
}
enum CommandState {
    Idle,
    Pending,
    Done,
}
type KeptValue = (Box<[u8]>, Box<[u8]>);

/// One page's asynchronous semantic face. Nodes are numbered locally.
pub struct BrowserFace<'a> {
    page: Token,
    pub(crate) first_url: String,
    pub(crate) origins: Vec<String>,
    address: std::borrow::Cow<'a, str>,
    serial: u64,
    requests: Vec<Request>,
    search: Option<Search>,
    answer: Option<Found>,
    seen: Vec<(Seen, Option<Role>)>,
    acting: Option<Token>,
    expected_path: Option<Box<[u8]>>,
    failure: Option<String>,
    trouble: Vec<Box<[u8]>>,
    warned: bool,
    go: Option<Token>,
    choice: Option<Choice>,
    keep: Option<Keeping>,
    kept: Option<KeptValue>,
    keep_done: bool,
    command: Option<crate::harness::CommandAction>,
    command_state: CommandState,
}

impl<'a> BrowserFace<'a> {
    #[must_use]
    pub fn new(page: Token, address: impl Into<std::borrow::Cow<'a, str>>) -> Self {
        Self {
            page,
            first_url: String::new(),
            origins: Vec::new(),
            address: address.into(),
            serial: page.raw().checked_mul(1_000_000).expect("page operation range fits"),
            requests: Vec::new(),
            search: None,
            answer: None,
            seen: Vec::new(),
            acting: None,
            expected_path: None,
            failure: None,
            trouble: Vec::new(),
            warned: false,
            go: None,
            choice: None,
            keep: None,
            kept: None,
            keep_done: false,
            command: None,
            command_state: CommandState::Idle,
        }
    }
    pub fn title(&mut self, words: &[u8]) -> Found {
        if let Some(answer) = self.answer.take() {
            return answer;
        }
        if self.search.is_none() {
            self.ask(Search {
                title: true,
                boxes: false,
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
    #[must_use]
    pub fn trouble(&self) -> &[Box<[u8]>] {
        &self.trouble
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
        self.search.is_some()
            || self.acting.is_some()
            || self.keep.is_some()
            || matches!(self.command_state, CommandState::Pending)
    }
    pub(crate) fn command(&mut self, action: crate::harness::CommandAction) {
        self.command = Some(action);
        self.command_state = CommandState::Pending;
    }
    pub(crate) fn command_action(&mut self) -> Option<crate::harness::CommandAction> {
        self.command.take()
    }
    pub(crate) fn command_complete(&mut self) {
        self.command_state = CommandState::Done;
    }
    pub(crate) fn command_done(&mut self) -> bool {
        if matches!(self.command_state, CommandState::Done) {
            self.command_state = CommandState::Idle;
            true
        } else {
            false
        }
    }
    pub(crate) fn navigating(&self) -> bool {
        self.acting.is_some()
    }
    pub(crate) fn failed(&self) -> bool {
        self.failure.is_some()
    }
    pub(crate) fn keep(&mut self, selector: Box<[u8]>, attribute: Option<Box<[u8]>>, name: Box<[u8]>) {
        let op = self.token();
        let attribute = attribute.unwrap_or_else(|| b"value".as_slice().into());
        self.requests.push(Request::Attribute {
            page: self.page,
            op,
            selector: selector.clone(),
            name: attribute.clone(),
        });
        self.keep = Some(Keeping { op, selector, attribute, name });
        self.keep_done = false;
    }
    pub(crate) fn kept(&mut self) -> Option<KeptValue> {
        self.kept.take()
    }
    pub(crate) fn kept_done(&mut self) -> bool {
        std::mem::take(&mut self.keep_done)
    }
    fn keep_failed(&mut self, why: &str) {
        let Keeping { selector, attribute, .. } = self.keep.take().expect("pending keep");
        self.fail(format!(
            "keep selector {} attribute {}: {why}",
            String::from_utf8_lossy(&selector),
            String::from_utf8_lossy(&attribute)
        ));
        self.keep_done = true;
    }
    pub(crate) fn fail(&mut self, why: String) {
        self.failure = Some(why);
    }
    pub fn failure(&mut self) -> Option<String> {
        self.failure.take()
    }
    pub(crate) fn text(&mut self, node: NodeId, words: Box<[u8]>, compose: bool) -> Result<(), String> {
        let seen = &self.seen.get(usize::try_from(node.0).expect("node fits")).ok_or("unknown field")?.0;
        let node = seen.node;
        let op = self.token();
        let request = if compose {
            Request::Compose { page: self.page, op, node, text: words }
        } else {
            Request::Type { page: self.page, op, node, text: words }
        };
        self.acting = Some(op);
        self.requests.push(request);
        Ok(())
    }
    pub(crate) fn find_boxed(&mut self, find: &Find) -> Found {
        if let Some(answer) = self.answer.take() {
            return answer;
        }
        if self.search.is_none() {
            self.ask(Search {
                title: false,
                boxes: true,
                find: find.clone(),
                depth: 0,
                role: 0,
                within: None,
                op: Token::new(0),
            });
        }
        self.answer.take().unwrap_or(Found::Pending)
    }
    pub(crate) fn height(&self, node: NodeId) -> Option<u64> {
        self.seen.get(usize::try_from(node.0).expect("node fits"))?.0.rect.map(|rect| rect.height)
    }
    pub(crate) fn press_at(&mut self, node: NodeId, at: skein_browser::boundary::Point) -> Result<(), String> {
        let seen = &self.seen.get(usize::try_from(node.0).expect("node fits")).ok_or("unknown drawing")?.0;
        let node = seen.node;
        let op = self.token();
        self.acting = Some(op);
        self.requests.push(Request::Press { page: self.page, op, node, at: Some(at) });
        Ok(())
    }
    pub(crate) fn key(&mut self, key: Key, shift: bool) {
        let op = self.token();
        self.acting = Some(op);
        self.requests.push(Request::Key { page: self.page, op, key, shift });
    }
    /// Read the page's address; the failure names it unless its path, query kept, is `path`.
    pub(crate) fn read_address(&mut self, path: &[u8]) {
        let op = self.token();
        self.acting = Some(op);
        self.expected_path = Some(path.into());
        self.requests.push(Request::Address { page: self.page, op });
    }
    pub(crate) fn focused(&self, node: NodeId) -> bool {
        self.seen.get(usize::try_from(node.0).expect("node fits")).is_some_and(|(seen, _)| seen.states.focused)
    }
    pub(crate) fn field_is(&self, node: NodeId, words: &[u8]) -> bool {
        self.seen
            .get(usize::try_from(node.0).expect("node fits"))
            .is_some_and(|(seen, _)| seen.states.focused && seen.value.as_ref() == words)
    }
    pub(crate) fn history(&mut self, forward: bool) {
        let op = self.token();
        self.warned = false;
        self.go = Some(op);
        self.acting = Some(op);
        self.requests.push(Request::Go { page: self.page, op, to: if forward { Go::Forward } else { Go::Back } });
    }
    pub(crate) fn reopen(&mut self) {
        self.absolute(self.first_url.as_bytes().into());
    }
    pub(crate) fn visit(&mut self, origin: u32, path: &[u8]) {
        if let Some(origin) = self.origins.get(usize::try_from(origin).expect("origin fits")) {
            self.absolute(format!("{origin}{}", String::from_utf8_lossy(path)).into_bytes().into());
        } else {
            self.fail(format!("unknown origin {origin}"));
        }
    }
    pub(crate) fn absolute(&mut self, url: Box<[u8]>) {
        let op = self.token();
        self.warned = false;
        self.go = Some(op);
        self.acting = Some(op);
        self.requests.push(Request::Go { page: self.page, op, to: Go::Address(url) });
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
            query: Query {
                role: Box::from(*role),
                name: name.into(),
                matching,
                within: search.within,
                boxes: search.boxes,
            },
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
            Event::Found { op, seen, more } => {
                if self.choice.is_some() && self.acting == Some(*op) {
                    self.choice_found(seen, *more);
                    return;
                }
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
                    self.choice_key();
                }
            }
            Event::Address { op, url } => {
                if self.acting == Some(*op) {
                    self.acting = None;
                    if let Some(path) = self.expected_path.take()
                        && url_path(url) != &*path
                    {
                        self.failure = Some(format!(
                            "the page's address was {}, expected the path {}",
                            String::from_utf8_lossy(url),
                            String::from_utf8_lossy(&path)
                        ));
                    }
                }
            }
            Event::Attribute { op, read } => {
                if self.keep.as_ref().map(|keep| keep.op) == Some(*op) {
                    match read {
                        Read::NoElement => self.keep_failed("no element"),
                        Read::NoAttribute => self.keep_failed("no attribute"),
                        Read::Value(value) => {
                            let Keeping { name, .. } = self.keep.take().expect("pending keep");
                            self.kept = Some((name, value.clone()));
                            self.keep_done = true;
                        }
                    }
                }
            }
            Event::Refused { op, why } => {
                if self.keep.as_ref().map(|keep| keep.op) == Some(*op) {
                    self.keep_failed(&format!("browser refused read: {why:?}"));
                }
                if self.search.as_ref().map(|s| s.op) == Some(*op) {
                    self.search = None;
                    self.answer = Some(Found::Absent);
                }
                if self.acting == Some(*op) {
                    self.acting = None;
                    self.choice = None;
                    self.failure = Some(format!("browser refused act: {why:?}"));
                }
            }
            Event::Trouble { page, text, .. } => {
                if *page == self.page {
                    self.trouble.push(text.clone());
                }
            }
            Event::Ready { .. }
            | Event::Opened { .. }
            | Event::Met { .. }
            | Event::Missed { .. }
            | Event::Snapshot { .. }
            | Event::Screenshot { .. }
            | Event::Looked { .. }
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
                let (seen, role) = self
                    .seen
                    .get(usize::try_from(node.0).expect("node fits"))
                    .ok_or_else(|| format!("unknown node {}", node.0))?;
                if *role == Some(Role::Option) && act == Act::Press {
                    let option = seen.node;
                    self.choice = Some(Choice {
                        option,
                        selects: std::collections::VecDeque::new(),
                        select: None,
                        keys: std::collections::VecDeque::new(),
                    });
                    self.choice_query(b"combobox", None);
                    return Ok(());
                }
                match act {
                    Act::Press | Act::Toggle => Request::Press { page: self.page, op, node: seen.node, at: None },
                    Act::Input { value } => {
                        if !seen.value.is_empty() {
                            return Err(format!(
                                "field '{}' holds words; the kit cannot replace them",
                                String::from_utf8_lossy(&seen.name)
                            ));
                        }
                        let keys = match seen.role.as_ref() {
                            b"Date" => Some(crate::keys::date(&value)),
                            b"InputTime" => Some(crate::keys::time(&value)),
                            _ => None,
                        };
                        if let Some(keys) = keys {
                            let text = keys.ok_or_else(|| {
                                format!(
                                    "field '{}' takes no value '{}'",
                                    String::from_utf8_lossy(&seen.name),
                                    String::from_utf8_lossy(&value)
                                )
                            })?;
                            Request::Keys { page: self.page, op, node: seen.node, text }
                        } else {
                            Request::Type { page: self.page, op, node: seen.node, text: value }
                        }
                    }
                    Act::Submit => Request::Key { page: self.page, op, key: Key::Enter, shift: false },
                    Act::Drawing { .. } => return Err("a person cannot press a drawing mark".into()),
                }
            }
        };
        self.acting = Some(op);
        self.requests.push(request);
        Ok(())
    }
    fn choice_query(&mut self, role: &[u8], within: Option<u64>) {
        let op = self.token();
        self.acting = Some(op);
        self.requests.push(Request::Find {
            page: self.page,
            op,
            query: Query { role: role.into(), name: Box::from([]), matching: Match::Words, within, boxes: false },
        });
    }
    fn choice_found(&mut self, seen: &skein_lib::List<Seen>, more: u32) {
        if more > 0 {
            self.choice = None;
            self.acting = None;
            self.fail("the option's select query exceeded the kit's bound".into());
            return;
        }
        let choice = self.choice.as_mut().expect("choice active");
        if let Some(select) = choice.select {
            if let Some(target) = seen.iter().position(|node| node.node == choice.option) {
                let current = seen.iter().position(|node| node.states.selected).unwrap_or(0);
                let key = if target < current { Key::ArrowUp } else { Key::ArrowDown };
                choice.keys.extend(std::iter::repeat_n(key, target.abs_diff(current)));
                choice.keys.push_back(Key::Enter);
                let op = self.token();
                self.acting = Some(op);
                self.requests.push(Request::Press { page: self.page, op, node: select, at: None });
                return;
            }
        } else {
            choice.selects.extend(seen.iter().map(|node| node.node));
        }
        if let Some(select) = choice.selects.pop_front() {
            choice.select = Some(select);
            self.choice_query(b"option", Some(select));
        } else {
            self.choice = None;
            self.acting = None;
            self.fail("the option no longer belongs to a select".into());
        }
    }
    fn choice_key(&mut self) {
        let Some(choice) = self.choice.as_mut() else { return };
        if let Some(key) = choice.keys.pop_front() {
            let op = self.token();
            self.acting = Some(op);
            self.requests.push(Request::Key { page: self.page, op, key, shift: false });
        } else {
            self.choice = None;
        }
    }
}
impl Face for BrowserFace<'_> {
    fn find(&mut self, find: &Find) -> Found {
        if let Some(answer) = self.answer.take() {
            return answer;
        }
        if self.search.is_none() {
            self.ask(Search {
                title: false,
                boxes: false,
                find: find.clone(),
                depth: 0,
                role: 0,
                within: None,
                op: Token::new(0),
            });
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

/// The URL from the first `/` after its `://`, query and fragment kept.
fn url_path(url: &[u8]) -> &[u8] {
    let rest =
        url.windows(3).position(|window| window == b"://").map_or(url, |at| url.get(at + 3..).unwrap_or_default());
    rest.iter().position(|byte| *byte == b'/').map_or(&[], |at| rest.get(at..).unwrap_or_default())
}
