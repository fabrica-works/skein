//! Waiting calls and returned answers (forge-connection.md, sections 2 and 8).
use crate::{
    boundary::{Answered, Event, Request},
    connection::{Connection, Phase},
    limits::{self, Limits, Shared, Unusable},
    settings::Settings,
    start,
    start::Start,
};
use skein_lib::{Env, Id, List, Map, Queue, Slab, Time, Token};
mod calls;
use skein_forge::api;
use skein_forge::forgejo::plans::{
    Body, Failure, Heard,
    plan::{Plan, Step},
    start::Read,
    tables::Tables,
};
/// Output reservations for each entry point.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MaxOut {
    pub above: u32,
    pub below: u32,
}
/// Reserve these slots before an entry point runs.
pub const MAX_OUT: MaxOut = MaxOut { above: 16, below: 16 };
/// Malformed answers and long bodies counted by the component.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Diagnostics {
    pub long_bodies: u64,
    pub malformed: u64,
}
#[derive(Debug)]
pub(crate) struct Waiting {
    pub repository: api::Repository,
    pub op: Option<api::Op>,
    pub plan: Option<Plan>,
    pub page_size: u32,
    pub connection: Option<Token>,
    pub cost: u32,
    pub charged: bool,
    pub deadline: Time,
}
#[derive(Debug)]
pub(crate) struct Held {
    pub answer: Answered,
    pub offer: bool,
}
/// One component per deployment, owning every configured token and start.
#[derive(Debug)]
pub struct Component {
    hooks: crate::hooks::Hooks,
    settings: Settings,
    starts: List<Start>,
    tables: Tables,
    waiting: Slab<Waiting>,
    calls: Map<Token, Id<Waiting>>,
    held: Map<Token, Held>,
    holders: Map<Token, Token>,
    closed: bool,
    diagnostics: Diagnostics,
    shared: Shared,
    connections: Map<Token, Id<Connection>>,
    connection_slab: Slab<Connection>,
    start_tokens: List<Option<Token>>,
    start_plans: List<Option<skein_forge::forgejo::plans::start::Plan>>,
    next_token: u64,
    wake: Option<Time>,
    ended: bool,
}
impl Component {
    pub fn new(settings: Settings, limits: &Limits, shared: &Shared) -> Result<Self, Unusable> {
        limits::check(limits, shared, &settings)?;
        let mut starts = List::with_capacity(limits.forges);
        let mut start_tokens = List::with_capacity(limits.forges);
        let mut start_plans = List::with_capacity(limits.forges);
        for _ in &settings.forges {
            starts.push(Start::Checking { next: limits.start_backoff }).expect("checked forge count");
            start_tokens.push(None).expect("checked forge count");
            start_plans.push(None).expect("checked forge count");
        }
        Ok(Self {
            hooks: crate::hooks::Hooks::new(limits),
            settings,
            starts,
            tables: Tables::new(limits.repositories, limits.users),
            waiting: Slab::with_capacity(shared.calls),
            calls: Map::with_capacity(shared.calls),
            held: Map::with_capacity(shared.calls),
            holders: Map::with_capacity(shared.calls),
            closed: false,
            diagnostics: Diagnostics { long_bodies: 0, malformed: 0 },
            shared: *shared,
            connections: Map::with_capacity(limits.connections),
            connection_slab: Slab::with_capacity(limits.connections),
            start_tokens,
            start_plans,
            next_token: 1,
            wake: None,
            ended: false,
        })
    }
    /// Opens each forge's start, within the connection and output bounds.
    pub fn begin(&mut self, env: &Env<Limits>, up: &mut Queue<Event>, io: &mut Queue<skein_io::Request>) {
        if self.closed {
            return;
        }
        let before = io.len();
        if let Some(settings) = &self.settings.webhooks {
            self.hooks.begin(settings, io);
        }
        for at in 0..self.starts.len() {
            *self.starts.get_mut(at).expect("configured start") = Start::Checking { next: env.limits.start_backoff };
        }
        self.drive_room(env, up, io, MAX_OUT.above, MAX_OUT.below.saturating_sub(io.len().saturating_sub(before)));
    }
    pub fn down(
        &mut self,
        env: &Env<Limits>,
        request: Request,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
    ) {
        if self.closed {
            return;
        }
        match request {
            Request::Call { call, repository, op } => {
                assert!(!self.calls.contains_key(&call), "one live call per token");
                let id = self
                    .waiting
                    .insert(Waiting {
                        repository,
                        op: Some(op),
                        plan: None,
                        page_size: 1,
                        connection: None,
                        cost: 0,
                        charged: false,
                        deadline: env.now.saturating_add(env.limits.deadline),
                    })
                    .expect("client bounds admitted calls");
                let _previous: Option<Id<Waiting>> = self.calls.insert(call, id).expect("call index matches its slab");
                self.drive(env, up, io, MAX_OUT.above);
            }
            Request::RefusedHint(hint) => self.hooks.refused(hint),
            Request::Refused(answer) => {
                drop(
                    self.held
                        .insert(answer.call, Held { answer, offer: true })
                        .expect("at most one held answer per client call"),
                );
            }
            Request::Returned(answer) => {
                drop(
                    self.held
                        .insert(answer.call, Held { answer, offer: false })
                        .expect("at most one held answer per client call"),
                );
            }
            Request::Offer => {
                self.hooks.offer();
                let before = up.len();
                if self.settings.webhooks.is_some() {
                    self.hooks.drive(env, &self.settings, up, io, MAX_OUT.above, MAX_OUT.below);
                }
                for _ in 0..MAX_OUT.above.saturating_sub(up.len().saturating_sub(before)) {
                    let mut next = None;
                    for (call, held) in &self.held {
                        if held.offer {
                            next = Some(*call);
                            break;
                        }
                    }
                    let Some(call) = next else {
                        break;
                    };
                    if let Some(held) = self.held.remove(&call) {
                        up.push(Event::Answered(held.answer));
                    }
                }
            }
            Request::Close => {
                for _ in 0..self.calls.len() {
                    let Some((call, id)) = self.calls.first() else {
                        break;
                    };
                    let (call, id) = (*call, *id);
                    self.waiting.retire(id);
                    let _removed: Option<Id<Waiting>> = self.calls.remove(&call);
                }
                self.held = Map::with_capacity(self.held.capacity());
                self.holders = Map::with_capacity(self.holders.capacity());
                self.closed = true;
                self.hooks.close();
                self.drive(env, up, io, MAX_OUT.above);
            }
        }
    }
    /// Routes io by this component's connection owner; stale owners are inert.
    pub fn up(
        &mut self,
        env: &Env<Limits>,
        event: skein_io::Event,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
    ) {
        let above = up.len();
        let below = io.len();
        let owner = match &event {
            skein_io::Event::Listening { owner, .. }
            | skein_io::Event::Accepted { owner, .. }
            | skein_io::Event::Stream { owner, .. }
            | skein_io::Event::Failed { owner, .. }
            | skein_io::Event::Closed { owner } => Some(*owner),
            skein_io::Event::Connecting { .. }
            | skein_io::Event::Connected { .. }
            | skein_io::Event::Output { .. }
            | skein_io::Event::Spawned { .. }
            | skein_io::Event::Exited { .. }
            | skein_io::Event::Usage { .. }
            | skein_io::Event::Resized
            | skein_io::Event::Window { .. }
            | skein_io::Event::Shutdown { .. } => None,
        };
        if let Some(owner) = owner
            && self.hooks.owns(owner)
        {
            self.hooks.up(env, event, up);
            self.drive_room(
                env,
                up,
                io,
                MAX_OUT.above.saturating_sub(up.len().saturating_sub(above)),
                MAX_OUT.below.saturating_sub(io.len().saturating_sub(below)),
            );
            return;
        }
        match event {
            skein_io::Event::Connecting { owner, socket } => {
                if let Some(id) = self.connections.get(&owner)
                    && let Some(c) = self.connection_slab.get_mut(*id)
                {
                    c.socket = Some(socket);
                }
            }
            skein_io::Event::Connected { owner } => {
                if let Some(id) = self.connections.get(&owner)
                    && let Some(c) = self.connection_slab.get_mut(*id)
                    && !c.closing
                {
                    c.connect(env);
                }
            }
            skein_io::Event::Stream { owner, up } => {
                if let Some(id) = self.connections.get(&owner)
                    && let Some(c) = self.connection_slab.get_mut(*id)
                {
                    c.stream(env, up);
                }
            }
            skein_io::Event::Failed { owner, .. } => {
                if let Some(id) = self.connections.get(&owner)
                    && let Some(c) = self.connection_slab.get_mut(*id)
                {
                    c.failed(Failure::Unseen);
                }
            }
            skein_io::Event::Closed { owner } => {
                if let Some(id) = self.connections.remove(&owner) {
                    let closing = self.connection_slab.get(id).expect("indexed connection").closing;
                    if let Some(call) = self.holders.remove(&owner)
                        && !self.closed
                    {
                        let call_id = *self.calls.get(&call).expect("holding call");
                        let c = self.connection_slab.get(id).expect("closed connection");
                        let error = if c.written { api::Error::Timeout } else { api::Error::Unavailable };
                        self.finish(call, call_id, Err(error), up);
                    }
                    self.connection_slab.retire(id);
                    for at in 0..self.start_tokens.len() {
                        if self.start_tokens.get(at) == Some(&Some(owner)) {
                            *self.start_tokens.get_mut(at).expect("configured forge") = None;
                            *self.start_plans.get_mut(at).expect("configured forge") = None;
                            if !self.closed && !closing {
                                self.start_heard(at, skein_forge::forgejo::plans::start::Heard::Unanswered, env, up);
                            }
                        }
                    }
                }
            }
            skein_io::Event::Listening { .. }
            | skein_io::Event::Accepted { .. }
            | skein_io::Event::Output { .. }
            | skein_io::Event::Spawned { .. }
            | skein_io::Event::Exited { .. }
            | skein_io::Event::Usage { .. }
            | skein_io::Event::Resized
            | skein_io::Event::Window { .. }
            | skein_io::Event::Shutdown { .. } => {}
        }
        self.drive(env, up, io, MAX_OUT.above.saturating_sub(up.len().saturating_sub(above)));
    }
    pub fn fire(&mut self, env: &Env<Limits>, up: &mut Queue<Event>, io: &mut Queue<skein_io::Request>) {
        if self.closed {
            self.drive(env, up, io, MAX_OUT.above);
            return;
        }
        let above = up.len();
        let below = io.len();
        for _ in 0..MAX_OUT.above {
            if io.len().saturating_sub(below) >= MAX_OUT.below {
                break;
            }
            let mut due = None;
            for (call, id) in &self.calls {
                if self.waiting.get(*id).expect("live call").deadline <= env.now {
                    due = Some((*call, *id));
                    break;
                }
            }
            let Some((call, id)) = due else {
                break;
            };
            self.expire(call, id, up, io);
        }
        self.drive_room(
            env,
            up,
            io,
            MAX_OUT.above.saturating_sub(up.len().saturating_sub(above)),
            MAX_OUT.below.saturating_sub(io.len().saturating_sub(below)),
        );
    }
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        let mut earliest = self.wake;
        if let Some(time) = self.hooks.deadline()
            && (earliest.is_none() || Some(time) < earliest)
        {
            earliest = Some(time);
        }
        for (_, id) in &self.connections {
            let c = self.connection_slab.get(*id).expect("indexed connection");
            if let Some(time) = c.deadline
                && (earliest.is_none() || Some(time) < earliest)
            {
                earliest = Some(time);
            }
        }
        if !self.closed {
            for state in &self.starts {
                if let Some((until, _)) = backoff(*state)
                    && (earliest.is_none() || Some(until) < earliest)
                {
                    earliest = Some(until);
                }
            }
        }
        for (_, id) in &self.calls {
            let time = self.waiting.get(*id).expect("indexed live call").deadline;
            if earliest.is_none() || Some(time) < earliest {
                earliest = Some(time);
            }
        }
        earliest
    }
    fn start_heard(
        &mut self,
        at: u32,
        heard: skein_forge::forgejo::plans::start::Heard,
        env: &Env<Limits>,
        up: &mut Queue<Event>,
    ) {
        let forge = self.settings.forges.get(at).expect("configured forge");
        let state = self.starts.get_mut(at).expect("configured start");
        let previous = *state;
        *state = start::heard(previous, heard, forge.user, env.now, &env.limits);
        if start_read(*state) == Some(Read::ApiSettings)
            && *state != previous
            && let Some(Some(token)) = self.start_tokens.get(at)
            && let Some(id) = self.connections.get(token)
            && let Some(c) = self.connection_slab.get_mut(*id)
        {
            c.deadline = Some(env.now.saturating_add(env.limits.deadline));
        }
        if let Some(reason) = stopped(*state)
            && *state != previous
        {
            up.push(Event::Stopped { forge: forge.number, why: reason });
        }
    }
    fn drive_starts(
        &mut self,
        env: &Env<Limits>,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
        above_room: u32,
        below_room: u32,
    ) {
        let below = io.len();
        let above = up.len();
        self.wake = None;
        for at in 0..self.starts.len() {
            if io.len().saturating_sub(below) >= below_room || up.len().saturating_sub(above) >= above_room {
                self.wake = Some(env.now);
                break;
            }
            let state = self.starts.get_mut(at).expect("configured start");
            if let Some((until, next)) = backoff(*state)
                && until <= env.now
                && !self.closed
            {
                *state = Start::Checking { next };
            }
            let mut token = *self.start_tokens.get(at).expect("configured slot");
            if !self.closed
                && token.is_none()
                && start_read(*state).is_some()
                && self.connections.len() < self.connections.capacity()
            {
                token = Some(self.open_start(at, env, io));
            }
            let Some(token) = token else {
                continue;
            };
            for _ in 0..64_u32 {
                if io.len().saturating_sub(below) >= below_room || up.len().saturating_sub(above) >= above_room {
                    self.wake = Some(env.now);
                    break;
                }
                let c = self
                    .connection_slab
                    .get_mut(*self.connections.get(&token).expect("indexed token"))
                    .expect("indexed connection");
                if self.closed {
                    *self.start_plans.get_mut(at).expect("configured forge") = None;
                    c.close(c.in_flight(), io);
                    break;
                }
                let state = *self.starts.get(at).expect("configured start");
                if let Some(deadline) = c.deadline
                    && deadline <= env.now
                {
                    if c.phase == Phase::Idle && ready(state) {
                        c.close(false, io);
                        break;
                    }
                    c.timeout();
                }
                if c.phase == Phase::Idle
                    && let Some(read) = start_read(state)
                {
                    let holder = self.start_plans.get_mut(at).expect("configured forge");
                    if holder.is_none() {
                        *holder = Some(skein_forge::forgejo::plans::start::Plan::new(read));
                    }
                    let plan = holder.as_mut().expect("active start read");
                    let skein_forge::forgejo::plans::start::Step::Send(exchange) = plan.begin() else {
                        unreachable!("a new start plan sends its read");
                    };
                    let forge = self.settings.forges.get(at).expect("configured forge");
                    c.start(env, forge, &self.settings.user_agent, exchange.request, exchange.range)
                        .expect("checked start request");
                }
                c.pump(env, io, below_room.saturating_sub(io.len().saturating_sub(below)).saturating_sub(1));
                if let Some(heard) = c.heard() {
                    let plan = self.start_plans.get_mut(at).expect("configured forge");
                    let step = match plan {
                        Some(plan) => plan.heard(heard),
                        None => None,
                    };
                    if let Some(skein_forge::forgejo::plans::start::Step::Done(heard)) = step {
                        *plan = None;
                        self.start_heard(at, heard, env, up);
                    }
                    let c = self
                        .connection_slab
                        .get_mut(*self.connections.get(&token).expect("indexed token"))
                        .expect("indexed connection");
                    if backoff(*self.starts.get(at).expect("configured start")).is_some()
                        || stopped(*self.starts.get(at).expect("configured start")).is_some()
                    {
                        c.close(true, io);
                    }
                    continue;
                }
                if c.phase == Phase::Closed {
                    c.close(c.in_flight(), io);
                }
                if !c.ready() {
                    break;
                }
                self.wake = Some(env.now);
            }
        }
    }
    fn drive(&mut self, env: &Env<Limits>, up: &mut Queue<Event>, io: &mut Queue<skein_io::Request>, above_room: u32) {
        self.drive_room(env, up, io, above_room, MAX_OUT.below);
    }
    fn drive_room(
        &mut self,
        env: &Env<Limits>,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
        above_room: u32,
        below_room: u32,
    ) {
        let above = up.len();
        let below = io.len();
        if self.settings.webhooks.is_some() {
            self.hooks.drive(env, &self.settings, up, io, above_room, below_room);
        }
        self.drive_starts(
            env,
            up,
            io,
            above_room.saturating_sub(up.len().saturating_sub(above)),
            below_room.saturating_sub(io.len().saturating_sub(below)),
        );
        self.drive_calls(
            env,
            up,
            io,
            above_room.saturating_sub(up.len().saturating_sub(above)),
            below_room.saturating_sub(io.len().saturating_sub(below)),
        );
        if self.closed
            && self.connections.is_empty()
            && self.hooks.closed()
            && !self.ended
            && up.len().saturating_sub(above) < above_room
        {
            self.ended = true;
            up.push(Event::Closed);
        }
    }
    fn open_start(&mut self, at: u32, env: &Env<Limits>, io: &mut Queue<skein_io::Request>) -> Token {
        assert!(self.next_token < 1_u64.checked_shl(59).expect("token prefix"), "connection token prefix");
        let owner = Token::new(self.next_token);
        self.next_token = self.next_token.checked_add(1).expect("connection token space");
        let forge = self.settings.forges.get(at).expect("configured forge");
        self.connection_slab.reclaim();
        let id = self
            .connection_slab
            .insert(Connection::new(forge, &env.limits, self.shared, env.now.saturating_add(env.limits.deadline)))
            .expect("free connection slot");
        let _previous = self.connections.insert(owner, id).expect("connection index has room");
        io.push(skein_io::Request::Connect { owner, addr: forge.address });
        *self.start_tokens.get_mut(at).expect("configured slot") = Some(owner);
        let plan = self.start_plans.get_mut(at).expect("configured forge");
        *plan = None;
        if let Some(read) = start_read(*self.starts.get(at).expect("configured start")) {
            *plan = Some(skein_forge::forgejo::plans::start::Plan::new(read));
        }
        owner
    }
    #[must_use]
    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }
    pub fn reclaim(&mut self) {
        self.waiting.reclaim();
        self.connection_slab.reclaim();
    }
    /// The bounded resolution tables owned by this component.
    pub fn tables(&mut self) -> &mut Tables {
        &mut self.tables
    }
    /// The pure start, in configured forge order.
    #[must_use]
    pub fn start(&self, forge: u16) -> Option<Start> {
        for (at, settings) in self.settings.forges.as_slice().iter().enumerate() {
            if settings.number == forge {
                return self.starts.as_slice().get(at).copied();
            }
        }
        None
    }
}

fn start_read(state: Start) -> Option<Read> {
    match state {
        Start::Checking { .. } => Some(Read::User),
        Start::Settings { .. } => Some(Read::ApiSettings),
        Start::Waiting { .. } | Start::Ready { .. } | Start::Stopped(_) => None,
    }
}
fn backoff(state: Start) -> Option<(Time, skein_lib::Duration)> {
    match state {
        Start::Waiting { until, next } => Some((until, next)),
        Start::Checking { .. } | Start::Settings { .. } | Start::Ready { .. } | Start::Stopped(_) => None,
    }
}
fn stopped(state: Start) -> Option<crate::Stopped> {
    match state {
        Start::Stopped(reason) => Some(reason),
        Start::Checking { .. } | Start::Settings { .. } | Start::Waiting { .. } | Start::Ready { .. } => None,
    }
}
fn ready(state: Start) -> bool {
    match state {
        Start::Ready { .. } => true,
        Start::Checking { .. } | Start::Settings { .. } | Start::Waiting { .. } | Start::Stopped(_) => false,
    }
}

#[cfg(test)]
mod tests;
