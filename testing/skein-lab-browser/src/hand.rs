//! One person's page and calls on a loopback listener (lab-browser.md, 8).
#[path = "hand/calls.rs"]
mod calls;
use super::Runtime;
use crate::{
    calls as capture,
    config::Config,
    json::{self, Value, number, object, string},
    redact::Redact,
};
use calls::{Call, Part};
use skein_browser::boundary::{self as b, Event, Request};
use skein_fake_person::{Doing, Face, Find, Found, Why};
use skein_fake_person_browser::BrowserFace;
use skein_io as io;
use skein_lib::{Duration, Time, Token, stream};
use skein_ui::{Act, NodeId, Role, takes};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    net::SocketAddr,
    os::unix::fs::OpenOptionsExt,
};
const OWNER: Token = Token::new(1 << 38);
const LINE: usize = 128 << 10;
const INTERRUPTED: &str = "termination requested";
struct Connection {
    socket: Token,
    number: u64,
    // lab-browser.md, 8.2: 10 seconds for its line from its turn, when
    // its reading starts; a queued connection's line waits unread.
    deadline: Option<Time>,
    bytes: Vec<u8>,
    reading: bool,
    complete: bool,
    reply: Option<Box<[u8]>>,
}
pub(super) struct Listener {
    address: Option<SocketAddr>,
    bound: Option<Token>,
    connections: BTreeMap<Token, Connection>,
    order: VecDeque<Token>,
    next: u64,
    accepted: u64,
    stopping: bool,
    pub(super) requests: VecDeque<io::Request>,
}
impl Listener {
    fn new(address: SocketAddr) -> Self {
        Self {
            address: None,
            bound: None,
            connections: BTreeMap::new(),
            order: VecDeque::new(),
            next: OWNER.raw() + 1,
            accepted: 0,
            stopping: false,
            requests: VecDeque::from([io::Request::Listen { owner: OWNER, addr: io::Addr::Ip(address) }]),
        }
    }
    pub(super) fn owns(&self, owner: Token) -> bool {
        owner == OWNER || self.connections.contains_key(&owner)
    }
    pub(super) fn event(&mut self, event: io::Event, now: Time) -> Result<(), String> {
        match event {
            io::Event::Listening { listener, addr, .. } => {
                self.bound = Some(listener);
                self.address = addr.socket();
            }
            io::Event::Accepted { socket, .. } => {
                self.accepted += 1;
                if self.stopping {
                    self.requests.push_back(io::Request::Reject { socket });
                } else {
                    let owner = Token::new(self.next);
                    self.next += 1;
                    self.connections.insert(
                        owner,
                        Connection {
                            socket,
                            number: self.accepted,
                            deadline: None,
                            bytes: Vec::new(),
                            reading: false,
                            complete: false,
                            reply: None,
                        },
                    );
                    self.order.push_back(owner);
                    self.requests.push_back(io::Request::Bind { socket, owner });
                }
            }
            io::Event::Stream { owner, up } => {
                let connection = self.connections.get_mut(&owner).ok_or("call connection missing")?;
                match up {
                    stream::Up::Bytes(bytes) => {
                        connection.bytes.extend_from_slice(&bytes);
                        if connection.bytes.len() > LINE + 1
                            || connection.deadline.is_some_and(|deadline| now >= deadline)
                        {
                            connection.complete = true;
                            connection.bytes.clear();
                            self.requests.push_back(io::Request::Abort { entity: connection.socket });
                        } else if connection.bytes.ends_with(b"\n") {
                            connection.complete = true;
                        } else {
                            connection.reading = false;
                        }
                    }
                    stream::Up::Room => {
                        if let Some(reply) = connection.reply.take() {
                            self.requests.push_back(io::Request::Stream {
                                stream: connection.socket,
                                down: stream::Down::Send(reply),
                            });
                            self.requests.push_back(io::Request::Close { entity: connection.socket });
                        }
                    }
                    stream::Up::End | stream::Up::Failed(_) => {
                        connection.complete = true;
                        connection.bytes.clear();
                        self.requests.push_back(io::Request::Abort { entity: connection.socket });
                    }
                }
            }
            io::Event::Closed { owner } => {
                if owner == OWNER {
                    self.bound = None;
                } else {
                    self.connections.remove(&owner);
                    self.order.retain(|queued| *queued != owner);
                }
            }
            io::Event::Failed { error, .. } => return Err(format!("call listener: {error:?}")),
            other @ (io::Event::Connecting { .. }
            | io::Event::Connected { .. }
            | io::Event::Output { .. }
            | io::Event::Spawned { .. }
            | io::Event::Exited { .. }
            | io::Event::Usage { .. }
            | io::Event::Resized
            | io::Event::Window { .. }
            | io::Event::Shutdown { .. }) => return Err(format!("unexpected call listener event: {other:?}")),
        }
        Ok(())
    }
    pub(super) fn fire(&mut self, now: Time) {
        for connection in self.connections.values_mut() {
            if !connection.complete && connection.deadline.is_some_and(|deadline| now >= deadline) {
                connection.complete = true;
                connection.bytes.clear();
                self.requests.push_back(io::Request::Abort { entity: connection.socket });
            }
        }
        if !self.stopping
            && let Some(connection) = self.order.front().and_then(|owner| self.connections.get_mut(owner))
            && !connection.reading
            && !connection.complete
        {
            connection.reading = true;
            connection.deadline.get_or_insert(now.saturating_add(Duration::from_secs(10)));
            self.requests.push_back(io::Request::Stream {
                stream: connection.socket,
                down: stream::Down::Demand {
                    read: stream::Read::Scan {
                        until: stream::Delimiter::new(b"\n").expect("LF"),
                        max: u32::try_from(LINE + 2 - connection.bytes.len()).expect("line bound"),
                    },
                    room: 0,
                },
            });
        }
    }
    fn call(&mut self) -> Option<(Token, u64, Vec<u8>)> {
        let owner = *self.order.front()?;
        let connection = self.connections.get_mut(&owner)?;
        if !connection.complete || !connection.bytes.ends_with(b"\n") {
            return None;
        }
        let mut bytes = std::mem::take(&mut connection.bytes);
        bytes.pop();
        Some((owner, connection.number, bytes))
    }
    fn answer(&mut self, owner: Token, bytes: Vec<u8>) {
        if let Some(connection) = self.connections.get_mut(&owner) {
            let room = u32::try_from(bytes.len()).expect("answer bound");
            connection.reply = Some(bytes.into());
            self.requests.push_back(io::Request::Stream {
                stream: connection.socket,
                down: stream::Down::Demand { read: stream::Read::Nothing, room },
            });
        }
    }
    pub(super) fn stop(&mut self) {
        self.stopping = true;
        if let Some(entity) = self.bound {
            self.requests.push_back(io::Request::Close { entity });
        }
        for connection in self.connections.values() {
            self.requests.push_back(io::Request::Abort { entity: connection.socket });
        }
    }
}
struct Trouble {
    kind: String,
    text: String,
    call: u64,
    during: Option<String>,
}
struct Evidence {
    log: File,
    written: u64,
    omitted: u64,
    summary_at: Option<u64>,
    trouble: Vec<Value>,
    more: u64,
}
impl Evidence {
    fn new(log: File) -> Self {
        Self { log, written: 0, omitted: 0, summary_at: None, trouble: Vec::new(), more: 0 }
    }
    fn take(&mut self) -> Value {
        object([
            ("trouble", Value::Array(std::mem::take(&mut self.trouble))),
            ("more", number(std::mem::take(&mut self.more))),
        ])
    }
    fn record(&mut self, trouble: Trouble, address: Value, redact: &Redact) -> Result<(), String> {
        let mut entry =
            object([("kind", string(&trouble.kind)), ("text", string(&trouble.text)), ("address", address.clone())]);
        redact.value(&mut entry);
        if self.trouble.len() < 64 {
            self.trouble.push(entry);
        } else {
            self.more += 1;
        }
        if self.written >= 4096 {
            self.omitted += 1;
            return self.finish_log(redact);
        }
        self.written += 1;
        let entry = object([
            ("n", number(self.written)),
            ("kind", string(trouble.kind)),
            ("text", string(trouble.text)),
            ("address", address),
            ("call", number(trouble.call)),
            ("during", trouble.during.map_or(Value::Null, string)),
        ]);
        self.log_line(entry, redact)
    }
    fn log_line(&mut self, mut value: Value, redact: &Redact) -> Result<(), String> {
        redact.value(&mut value);
        let mut bytes = json::encode(&value);
        bytes.push(b'\n');
        self.log.write_all(&bytes).map_err(|e| e.to_string())
    }
    fn finish_log(&mut self, redact: &Redact) -> Result<(), String> {
        if self.omitted != 0 {
            let at = if let Some(at) = self.summary_at {
                at
            } else {
                let at = self.log.stream_position().map_err(|e| e.to_string())?;
                self.summary_at = Some(at);
                at
            };
            self.log.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
            self.log_line(
                object([
                    ("n", number(self.written + 1)),
                    ("kind", string("limit")),
                    ("text", string(format!("{} entries not written", self.omitted))),
                    ("address", Value::Null),
                    ("call", number(0)),
                    ("during", Value::Null),
                ]),
                redact,
            )?;
            let end = self.log.stream_position().map_err(|e| e.to_string())?;
            self.log.set_len(end).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
struct Hand {
    rt: Runtime,
    events: VecDeque<Event>,
    page: Token,
    serial: u64,
    redact: Redact,
    links: usize,
    shots: u64,
    evidence: Evidence,
    last_refusal: Option<b::Refusal>,
    failed: Option<String>,
    address: Value,
    addresses: BTreeMap<Token, (Trouble, Option<Value>)>,
    call: u64,
    during: Option<String>,
}
pub(crate) fn run(config: &Config) -> Result<(), String> {
    let mut hand = Hand::new(config)?;
    let result = hand.run(config);
    let result = if hand.rt.interrupted { hand.shutdown() } else { result };
    result.map_err(|error| hand.redact.text(&error))
}
impl Hand {
    fn new(config: &Config) -> Result<Self, String> {
        let options = config.hand.as_ref().expect("hand form");
        std::fs::create_dir_all(&config.out).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(config.scratch.join("fontconfig")).map_err(|e| e.to_string())?;
        let fonts = config.scratch.join("fonts.conf");
        std::fs::write(
            &fonts,
            crate::config::fonts(
                config.fonts.to_str().ok_or("fonts path must be UTF-8")?,
                config.scratch.to_str().ok_or("scratch path must be UTF-8")?,
            ),
        )
        .map_err(|e| e.to_string())?;
        let log = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(config.out.join("trouble.jsonl"))
            .map_err(|e| e.to_string())?;
        let mut rt = Runtime::new(true)?;
        rt.listener = Some(Listener::new(options.listen));
        rt.proxy.for_hand();
        while rt.proxy.address.is_none() || rt.listener.as_ref().and_then(|l| l.address).is_none() {
            rt.pump()?;
        }
        rt.spawn(config, &fonts)?;
        let ready = rt.next_event()?;
        let Event::Ready { .. } = ready else {
            return Err(format!("expected Ready: {ready:?}"));
        };
        rt.open("hand".into(), options.width, options.height, options.mobile)?;
        let page = rt.pages.get("hand").expect("opened page").page;
        let address = rt.listener.as_ref().and_then(|l| l.address).expect("bound listener");
        rt.write_raw(format!("listening {address}\n").into_bytes())?;
        Ok(Self {
            rt,
            events: VecDeque::new(),
            page,
            serial: 1 << 48,
            redact: Redact::default(),
            links: 0,
            shots: 0,
            evidence: Evidence::new(log),
            last_refusal: None,
            failed: None,
            address: string("about:blank"),
            addresses: BTreeMap::new(),
            call: 0,
            during: None,
        })
    }
    fn run(&mut self, config: &Config) -> Result<(), String> {
        loop {
            self.events.clear();
            self.pump()?;
            let Some((owner, number, line)) = self.rt.listener.as_mut().expect("listener").call() else {
                continue;
            };
            self.events.clear();
            self.last_refusal = None;
            self.call = number;
            let parsed = calls::parse(&line);
            self.during = parsed.as_ref().ok().map(|call| call.name().to_owned());
            let mut answer = match parsed {
                Ok(call) => {
                    let options = config.hand.as_ref().expect("hand form");
                    let mut face = BrowserFace::continuing(self.page, options.authority.clone(), self.serial);
                    let answer = self.call(call, &mut face, config);
                    while let Some(event) = self.events.pop_front() {
                        face.event(&event);
                    }
                    self.serial = face.serial();
                    answer?
                }
                Err(answer) => answer,
            };
            self.redact.value(&mut answer);
            let mut bytes = json::encode(&answer);
            bytes.push(b'\n');
            self.rt.listener.as_mut().expect("listener").answer(owner, bytes);
            while self.rt.listener.as_ref().expect("listener").connections.contains_key(&owner) {
                self.pump()?;
            }
            self.call = 0;
            self.during = None;
        }
    }
    fn pump(&mut self) -> Result<(), String> {
        self.rt.pump().inspect_err(|error| self.failed = Some(error.clone()))?;
        if self.rt.interrupted && !self.rt.closing {
            return Err(INTERRUPTED.into());
        }
        while let Some(event) = self.rt.answers.pop_front() {
            if let Event::Trouble { page, trouble, text } = &event
                && *page == self.page
            {
                let kind = match trouble {
                    b::Trouble::Exception => "exception",
                    b::Trouble::Console => "console",
                    b::Trouble::Log => "log",
                    b::Trouble::Crash => "crash",
                    b::Trouble::Dialog => "dialog",
                };
                self.start_trouble(kind, String::from_utf8_lossy(text).into_owned());
            }
            if let Event::Address { op, url } = &event {
                self.address = json::bytes(url);
                if let Some((_, address)) = self.addresses.get_mut(op) {
                    *address = Some(self.address.clone());
                }
            }
            if let Event::Refused { op, why } = &event {
                if let Some((_, address)) = self.addresses.get_mut(op) {
                    *address = Some(self.address.clone());
                } else if op.raw() > (1 << 48) {
                    self.last_refusal = Some(*why);
                }
            }
            self.events.push_back(event);
        }
        while self.addresses.first_key_value().is_some_and(|(_, (_, address))| address.is_some()) {
            let (_, (trouble, address)) = self.addresses.pop_first().expect("completed address");
            self.evidence.record(trouble, address.expect("completed address"), &self.redact)?;
        }
        for host in self.rt.proxy.take() {
            self.start_trouble("network", host);
        }
        Ok(())
    }
    fn start_trouble(&mut self, kind: &str, text: String) {
        let op = self.rt.fresh();
        let mut text = text;
        let mut end = text.len().min(usize::try_from(self.rt.benv.limits.text).expect("text limit"));
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        self.addresses
            .insert(op, (Trouble { kind: kind.into(), text, call: self.call, during: self.during.clone() }, None));
        self.rt.ask(Request::Address { page: self.page, op });
    }
    fn face(&mut self, face: &mut BrowserFace<'_>) -> Result<(), String> {
        loop {
            for request in face.requests() {
                self.rt.ask(request);
            }
            while let Some(event) = self.events.pop_front() {
                face.event(&event);
            }
            if !face.pending() {
                if face.refusal().or(self.last_refusal).is_some() {
                    return Err("browser refused".into());
                }
                if let Some(error) = face.failure() {
                    return Err(error);
                }
                return Ok(());
            }
            self.pump()?;
        }
    }
    fn find(&mut self, face: &mut BrowserFace<'_>, find: &Find) -> Result<Option<NodeId>, String> {
        loop {
            match face.find(find) {
                Found::Node(node) => return Ok(Some(node)),
                Found::Absent => return Ok(None),
                Found::Pending => self.face(face)?,
            }
        }
    }
    fn required(&mut self, face: &mut BrowserFace<'_>, find: &Find) -> Result<NodeId, Value> {
        self.find(face, find)
            .map_err(|error| self.failure(error, face))?
            .ok_or_else(|| capture::refused("missing", Why::Missing { find: find.clone() }.to_string()))
    }
    fn failure(&self, error: String, face: &BrowserFace<'_>) -> Value {
        face.refusal().or(self.last_refusal).map_or_else(|| capture::refused("call", error), capture::kit_refusal)
    }
    fn failed_answer(&self, error: String, face: &BrowserFace<'_>) -> Result<Value, String> {
        if self.rt.interrupted { Err(INTERRUPTED.into()) } else { Ok(self.failure(error, face)) }
    }
    fn wait_event(&mut self, op: Token, face: &mut BrowserFace<'_>) -> Result<Event, String> {
        loop {
            while let Some(event) = self.events.pop_front() {
                face.event(&event);
                if event_op(&event) == Some(op) && !matches!(event, Event::Warned { .. }) {
                    return Ok(event);
                }
            }
            self.pump()?;
        }
    }
    fn address(&mut self, face: &mut BrowserFace<'_>) -> Result<Value, String> {
        face.address();
        self.face(face)?;
        Ok(json::bytes(&face.take_address().ok_or("address missing")?))
    }
    fn call(&mut self, call: Call, face: &mut BrowserFace<'_>, config: &Config) -> Result<Value, String> {
        let answer = self.answer(call, face, config);
        if let Some(error) = self.failed.take() {
            return Err(error);
        }
        if self.rt.interrupted {
            return Err(INTERRUPTED.into());
        }
        answer
    }
    #[expect(clippy::too_many_lines, reason = "the closed call table keeps each verb beside its typed answer")]
    fn answer(&mut self, call: Call, face: &mut BrowserFace<'_>, config: &Config) -> Result<Value, String> {
        let options = config.hand.as_ref().expect("hand form");
        match call {
            Call::Go(path) => {
                if !calls::path(&path) {
                    return Ok(capture::refused("origin", "path must be on the origin"));
                }
                face.go_to(format!("{}{path}", options.origin).into_bytes().into());
                self.moved(face, "went")
            }
            Call::SignIn(link) => {
                let query = match calls::link(&options.origin, &link, self.links) {
                    Ok(query) => query,
                    Err(answer) => return Ok(answer),
                };
                self.redact.keep(query);
                self.links += 1;
                face.go_to(link.into_bytes().into());
                self.moved(face, "opened")
            }
            Call::History(to) => {
                face.history(to);
                self.moved(face, "went")
            }
            Call::Address => match self.address(face) {
                Ok(address) => Ok(object([("address", address)])),
                Err(error) => Ok(self.failure(error, face)),
            },
            Call::Key(key, shift) => {
                face.key(key, shift);
                if let Err(error) = self.face(face) {
                    return self.failed_answer(error, face);
                }
                let name = match key {
                    b::Key::Enter => "Enter",
                    b::Key::Escape => "Escape",
                    b::Key::Tab => "Tab",
                    b::Key::Backspace
                    | b::Key::ArrowUp
                    | b::Key::ArrowDown
                    | b::Key::ArrowLeft
                    | b::Key::ArrowRight
                    | b::Key::SelectAll => unreachable!("parsed key"),
                };
                Ok(object([("keyed", string(format!("{}{name}", if shift { "Shift+" } else { "" })))]))
            }
            Call::Press(part) => {
                let act = if takes(part.role, false, &Act::Toggle) && !takes(part.role, false, &Act::Press) {
                    Act::Toggle
                } else {
                    Act::Press
                };
                self.act(face, &part, act, "pressed", false)
            }
            Call::Type(part, text) => {
                self.act(face, &part, Act::Input { value: text.into_bytes().into() }, "typed", true)
            }
            Call::Choose(part, option) => {
                if part.role != Role::Select {
                    return Ok(capture::refused("takes", "part is not a select"));
                }
                if let Err(answer) = self.required(face, &part.find) {
                    return Ok(answer);
                }
                let mut within = part.find.within.to_vec();
                within.push(part.find.target);
                let find = if option.exact {
                    Find::exactly_named(Role::Option, option.name.as_bytes())
                } else {
                    Find::named(Role::Option, option.name.as_bytes())
                }
                .within(within.into());
                self.act(face, &Part { role: Role::Option, find }, Act::Press, "chose", false)
            }
            Call::Wait(part, gone, seconds) => self.wait(face, &part, gone, seconds),
            Call::Outline(scope) => {
                let root = if let Some((last, parents)) = scope.split_last() {
                    let find = Find { target: last.clone(), within: parents.into() };
                    let node = match self.required(face, &find) {
                        Ok(node) => node,
                        Err(answer) => return Ok(answer),
                    };
                    Some(face.seen(node).expect("found node").node)
                } else {
                    None
                };
                let op = self.rt.fresh();
                self.rt.ask(Request::Look { page: self.page, op, look: b::Look::Tree });
                let event = self.wait_event(op, face)?;
                if let Event::Refused { why, .. } = event {
                    return Ok(capture::kit_refusal(why));
                }
                let Event::Looked { looked: b::Looked::Tree(entries), .. } = event else {
                    return Err(format!("outline answer: {event:?}"));
                };
                let (text, left) = crate::outline::outline(entries.as_slice(), root);
                Ok(object([("outline", string(text)), ("left", number(left))]))
            }
            Call::Shot => {
                let op = self.rt.fresh();
                self.rt.ask(Request::Screenshot { page: self.page, op });
                let event = self.wait_event(op, face)?;
                if let Event::Refused { why, .. } = event {
                    return Ok(capture::kit_refusal(why));
                }
                let Event::Screenshot { png, .. } = event else {
                    return Err(format!("shot answer: {event:?}"));
                };
                self.shots += 1;
                super::shot(config, &format!("shots/{:04}.png", self.shots), &png)
            }

            Call::Trouble => {
                while !self.addresses.is_empty() {
                    self.pump()?;
                }
                Ok(self.evidence.take())
            }
        }
    }
    fn shutdown(&mut self) -> Result<(), String> {
        self.rt.closing = true;
        self.rt.listener.as_mut().expect("listener").stop();
        self.rt.ask(Request::Close { entity: self.page });
        self.closed(self.page)?;
        let person = self.rt.pages.get("hand").expect("person").person;
        self.rt.ask(Request::Close { entity: person });
        self.closed(person)?;
        while !self.addresses.is_empty() {
            self.pump()?;
            self.events.clear();
        }
        self.rt.ask(Request::Close { entity: super::BROWSER });
        self.closed(super::BROWSER)?;
        self.rt.proxy.stop();
        if let Some(entity) = self.rt.signals.take() {
            self.rt.requests.push_back(io::Request::Close { entity });
        }
        self.rt.requests.push_back(io::Request::Close { entity: self.rt.output });
        let deadline = self.rt.clock.now().now.saturating_add(Duration::from_secs(8));
        while !(self.rt.exited
            && self.rt.child_closed
            && self.rt.root_closed
            && self.rt.io.is_empty()
            && self.rt.ring.is_empty()
            && self.rt.requests.is_empty()
            && self.rt.submits.is_empty())
        {
            if self.rt.clock.now().now >= deadline {
                return Err("shutdown deadline passed".into());
            }
            self.pump()?;
            self.events.clear();
        }
        self.evidence.finish_log(&self.redact)
    }
    fn closed(&mut self, entity: Token) -> Result<(), String> {
        let deadline = self.rt.clock.now().now.saturating_add(Duration::from_secs(10));
        loop {
            while let Some(event) = self.events.pop_front() {
                if let Event::Closed { owner } = event
                    && owner == entity
                {
                    return Ok(());
                }
            }
            if self.rt.clock.now().now >= deadline {
                return Err("close deadline passed".into());
            }
            self.pump()?;
        }
    }
    fn moved(&mut self, face: &mut BrowserFace<'_>, field: &str) -> Result<Value, String> {
        if let Err(error) = self.face(face) {
            return self.failed_answer(error, face);
        }
        match self.address(face) {
            Ok(address) => Ok(object([(field, address)])),
            Err(error) => self.failed_answer(error, face),
        }
    }
    fn act(
        &mut self,
        face: &mut BrowserFace<'_>,
        part: &Part,
        act: Act,
        field: &str,
        reread: bool,
    ) -> Result<Value, String> {
        if !takes(part.role, false, &act) {
            return Ok(capture::refused("takes", "role does not take this act"));
        }
        let node = match self.required(face, &part.find) {
            Ok(node) => node,
            Err(answer) => return Ok(answer),
        };
        let seen = calls::part(part.role, face.seen(node).expect("found node"));
        if let Err(error) = face.perform(Doing::Act { node, act }).and_then(|()| self.face(face)) {
            return self.failed_answer(error, face);
        }
        let seen = if reread {
            let node = match self.required(face, &part.find) {
                Ok(node) => node,
                Err(answer) => return Ok(answer),
            };
            calls::part(part.role, face.seen(node).expect("found node"))
        } else {
            seen
        };
        Ok(object([(field, seen)]))
    }
    fn wait(&mut self, face: &mut BrowserFace<'_>, part: &Part, gone: bool, seconds: u32) -> Result<Value, String> {
        let deadline = self.rt.ienv.now.saturating_add(Duration::from_secs(u64::from(seconds)));
        loop {
            let found = match self.find(face, &part.find) {
                Ok(found) => found,
                Err(error) => return Ok(self.failure(error, face)),
            };
            if gone && found.is_none() {
                return Ok(object([("gone", Value::Bool(true))]));
            }
            if !gone && let Some(node) = found {
                return Ok(object([("met", calls::part(part.role, face.seen(node).expect("found node")))]));
            }
            if self.rt.ienv.now >= deadline {
                break;
            }
            let next = self.rt.ienv.now.saturating_add(self.rt.benv.limits.poll).min(deadline);
            while self.rt.ienv.now < next {
                self.pump()?;
            }
        }
        let find = Find::named(part.role, b"").within(part.find.within.clone());
        // Resolve the scope through the face, then use its kit query's whole
        // answer for the bounded diagnostic, including the first eight parts.
        let mut observed = Vec::new();
        let mut target_ops = Vec::new();
        let mut depth = 0;
        loop {
            match face.find(&find) {
                Found::Node(_) | Found::Absent => break,
                Found::Pending => {
                    for request in face.requests() {
                        if let Request::Find { op, .. } = &request
                            && depth >= find.within.len()
                        {
                            target_ops.push(*op);
                        }
                        self.rt.ask(request);
                    }
                    self.pump()?;
                    while let Some(event) = self.events.pop_front() {
                        if let Event::Found { op, seen, .. } = &event {
                            if target_ops.contains(op) {
                                observed.extend(seen.iter().take(8).map(|seen| calls::part(part.role, seen)));
                            } else if !seen.is_empty() {
                                depth += 1;
                            }
                        }
                        face.event(&event);
                    }
                }
            }
        }
        if let Some(why) = face.refusal().or(self.last_refusal) {
            return Ok(capture::kit_refusal(why));
        }
        observed.truncate(8);
        Ok(object([("missed", Value::Array(observed))]))
    }
}
fn event_op(event: &Event) -> Option<Token> {
    match event {
        Event::Found { op, .. }
        | Event::Met { op, .. }
        | Event::Missed { op, .. }
        | Event::Warned { op }
        | Event::Done { op }
        | Event::Refused { op, .. }
        | Event::Address { op, .. }
        | Event::Attribute { op, .. }
        | Event::Snapshot { op, .. }
        | Event::Screenshot { op, .. }
        | Event::Looked { op, .. } => Some(*op),
        Event::Ready { .. } | Event::Opened { .. } | Event::Trouble { .. } | Event::Closed { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn accept(listener: &mut Listener, socket: u64, now: Time) -> Token {
        listener
            .event(
                io::Event::Accepted {
                    owner: OWNER,
                    socket: Token::new(socket),
                    peer: io::Addr::Ip("127.0.0.1:1234".parse().unwrap()),
                },
                now,
            )
            .unwrap();
        *listener.order.back().unwrap()
    }
    fn bytes(listener: &mut Listener, owner: Token, bytes: &[u8], now: Time) {
        listener.event(io::Event::Stream { owner, up: stream::Up::Bytes(bytes.into()) }, now).unwrap();
    }
    #[test]
    fn listener_reads_in_accept_order_until_answer_is_written_and_closed() {
        let mut listener = Listener::new("127.0.0.1:0".parse().unwrap());
        let first = accept(&mut listener, 1, Time::ZERO);
        let second = accept(&mut listener, 2, Time::ZERO);
        listener.requests.clear();
        listener.fire(Time::ZERO);
        assert!(listener.connections[&first].reading);
        assert!(!listener.connections[&second].reading);
        bytes(&mut listener, first, b"{}\n", Time::ZERO);
        let (owner, n, line) = listener.call().unwrap();
        assert_eq!((owner, n, line), (first, 1, b"{}".to_vec()));
        listener.answer(first, b"{}\n".to_vec());
        listener.event(io::Event::Stream { owner: first, up: stream::Up::Room }, Time::ZERO).unwrap();
        listener.fire(Time::ZERO);
        assert!(!listener.connections[&second].reading);
        listener.event(io::Event::Closed { owner: first }, Time::ZERO).unwrap();
        listener.fire(Time::ZERO);
        assert!(listener.connections[&second].reading);
    }
    #[test]
    fn listener_line_limit_and_deadline_from_its_turn_close_unanswered() {
        for (length, accepted) in [(LINE, true), (LINE + 1, false)] {
            let mut listener = Listener::new("127.0.0.1:0".parse().unwrap());
            let owner = accept(&mut listener, 1, Time::ZERO);
            let mut line = vec![b'x'; length];
            line.push(b'\n');
            bytes(&mut listener, owner, &line, Time::ZERO);
            assert_eq!(listener.call().is_some(), accepted);
        }
        let mut listener = Listener::new("127.0.0.1:0".parse().unwrap());
        let first = accept(&mut listener, 1, Time::ZERO);
        let second = accept(&mut listener, 2, Time::ZERO);
        let at = |millis| Time::ZERO.saturating_add(Duration::from_millis(millis));
        listener.fire(at(0));
        listener.fire(at(9999));
        assert!(!listener.connections[&first].complete);
        listener.fire(at(10_000));
        assert!(listener.connections[&first].complete);
        assert!(listener.call().is_none());
        assert!(
            !listener.connections[&second].complete && !listener.connections[&second].reading,
            "a queued connection keeps its line unread past the first's expiry"
        );
        listener.event(io::Event::Closed { owner: first }, at(10_000)).unwrap();
        listener.fire(at(10_000));
        assert!(listener.connections[&second].reading, "its turn starts its 10 seconds");
        bytes(&mut listener, second, b"{}\n", at(10_001));
        assert_eq!(listener.call().map(|(owner, n, _)| (owner, n)), Some((second, 2)));
        let mut listener = Listener::new("127.0.0.1:0".parse().unwrap());
        let only = accept(&mut listener, 1, at(0));
        listener.fire(at(5000));
        listener.fire(at(14_999));
        assert!(!listener.connections[&only].complete);
        listener.fire(at(15_000));
        assert!(listener.connections[&only].complete);
        assert!(listener.call().is_none());
    }
    #[test]
    fn trouble_log_and_memory_limits_redaction_and_drain() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = skein_scratch::Scratch::new("hand-trouble");
        let path = scratch.path().join("trouble.jsonl");
        let file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path).unwrap();
        let mut evidence = Evidence::new(file);
        let mut redact = Redact::default();
        redact.keep("abcdefghijklmnop");
        for n in 0..4098 {
            evidence
                .record(
                    Trouble {
                        kind: "console".into(),
                        text: "abcdefghijklmnop".into(),
                        call: 7,
                        during: Some("sign-in".into()),
                    },
                    string("http://127.0.0.1:41/?abcdefghijklmnop"),
                    &redact,
                )
                .unwrap();
            if n == 63 {
                assert_eq!(evidence.trouble.len(), 64);
                assert_eq!(evidence.more, 0);
            }
            if n == 64 {
                assert_eq!(evidence.trouble.len(), 64);
                assert_eq!(evidence.more, 1);
            }
        }
        let answer = evidence.take();
        let Value::Object(fields) = answer else { panic!("trouble answer") };
        assert_eq!(fields.get("more"), Some(&number(4034)));
        assert_eq!(evidence.take(), object([("trouble", Value::Array(vec![])), ("more", number(0))]));
        let log = std::fs::read_to_string(&path).unwrap();
        assert!(!log.contains("abcdefghijklmnop"));
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines.len(), 4097);
        let first = json::parse(lines[0].as_bytes()).unwrap();
        assert_eq!(
            first,
            object([
                ("n", number(1)),
                ("kind", string("console")),
                ("text", string("<link>")),
                ("address", string("http://127.0.0.1:41/?<link>")),
                ("call", number(7)),
                ("during", string("sign-in"))
            ])
        );
        let last = json::parse(lines[4096].as_bytes()).unwrap();
        assert_eq!(
            last,
            object([
                ("n", number(4097)),
                ("kind", string("limit")),
                ("text", string("2 entries not written")),
                ("address", Value::Null),
                ("call", number(0)),
                ("during", Value::Null)
            ])
        );
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
