//! The host component: the plans run inside a service (forge-setup.md, 6.3).
mod boundary;
mod connection;
mod limits;
pub(crate) mod run;
mod settings;
use crate::reconcile::{self, Failure, Reply, Stage, Why};
pub use boundary::{Answer, Event, MAX_OUT, MaxOut, Need, Plan, Request};
use connection::Connection;
pub use limits::{Limits, Stacked, Unusable, documents, http};
use run::{Run, Send};
pub use settings::{Forge, Kind, Settings, Transport};
use skein_lib::{CryptoRng, Env, List, Map, Queue, Secret, Time, Token, bytes};

/// A service's setup plans, with one run per forge and bounded connections.
pub struct Host {
    settings: Settings,
    crypto: CryptoRng,
    runs: Map<Token, Run>,
    connections: Map<Token, Connection>,
    plan_out: Queue<reconcile::Need>,
    closing_tokens: List<Token>,
    next_token: u64,
    wake: Option<Time>,
    closing: bool,
    ended: bool,
}
impl Host {
    pub fn new(limits: &Limits, settings: Settings, crypto: CryptoRng) -> Result<Self, Unusable> {
        limits::check(limits, &settings)?;
        Ok(Self {
            settings,
            crypto,
            runs: Map::with_capacity(limits.runs),
            connections: Map::with_capacity(limits.runs),
            plan_out: Queue::with_capacity(reconcile::MAX_OUT),
            closing_tokens: List::with_capacity(limits.runs),
            next_token: 1,
            wake: None,
            closing: false,
            ended: false,
        })
    }
    pub fn down(
        &mut self,
        env: &Env<Limits>,
        request: Request,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
    ) {
        if self.closing {
            return;
        }
        let above = up.len();
        match request {
            Request::Run { run, plan } => {
                let forge = plan.forge();
                assert!(!self.runs.contains_key(&run), "one live run per owner token");
                let given = &configured(&self.settings, forge).given;
                let mut busy = self.runs.len() == self.runs.capacity();
                for (_, live) in &self.runs {
                    busy |= live.forge == forge;
                }
                if busy {
                    up.push(Event::Stop {
                        run,
                        failure: Failure {
                            forge,
                            step: Stage::Admin,
                            subject: given.admin_name.clone(),
                            why: Why::Busy,
                        },
                    });
                } else {
                    let mut live = Run::new(given, plan, env.limits.token_bytes, env.now, &mut self.plan_out);
                    match live.pending.take() {
                        Some(reconcile::Need::Stop(failure)) => up.push(Event::Stop { run, failure }),
                        Some(reconcile::Need::Done(outcome)) => up.push(Event::Done { run, outcome }),
                        pending @ (Some(
                            reconcile::Need::Read(_)
                            | reconcile::Need::Write { .. }
                            | reconcile::Need::Send { .. }
                            | reconcile::Need::Draw(_),
                        )
                        | None) => {
                            live.pending = pending;
                            let previous = self.runs.insert(run, live).expect("admitted run");
                            assert!(previous.is_none(), "fresh owner token");
                        }
                    }
                }
            }
            Request::Answer { run, answer } => {
                if let Some(live) = self.runs.get_mut(&run) {
                    live.answer(answer, env.limits.token_bytes, &mut self.plan_out);
                }
            }
            Request::Close => {
                self.closing = true;
                self.runs = Map::with_capacity(self.runs.capacity());
                self.closing_tokens.clear();
                for (token, _) in &self.connections {
                    self.closing_tokens.push(*token).expect("bounded connections");
                }
                for token in &self.closing_tokens {
                    self.connections.get_mut(token).expect("owned connection").abort();
                }
            }
        }
        self.drive(env, up, io, MAX_OUT.above.saturating_sub(up.len().saturating_sub(above)), MAX_OUT.below);
    }
    pub fn up(
        &mut self,
        env: &Env<Limits>,
        event: skein_io::Event,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
    ) {
        match event {
            skein_io::Event::Connecting { owner, socket } => {
                if let Some(c) = self.connections.get_mut(&owner) {
                    c.socket = Some(socket);
                }
            }
            skein_io::Event::Connected { owner } => {
                if let Some(c) = self.connections.get_mut(&owner) {
                    c.connect(env);
                }
            }
            skein_io::Event::Stream { owner, up } => {
                if let Some(c) = self.connections.get_mut(&owner) {
                    c.stream(env, up);
                }
            }
            skein_io::Event::Failed { owner, .. } => {
                if let Some(c) = self.connections.get_mut(&owner) {
                    c.fail();
                }
            }
            skein_io::Event::Closed { owner } => {
                if let Some(c) = self.connections.get_mut(&owner) {
                    c.closed = true;
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
            | skein_io::Event::Shutdown { .. } => (),
        }
        self.drive(env, up, io, MAX_OUT.above, MAX_OUT.below);
    }
    pub fn fire(&mut self, env: &Env<Limits>, up: &mut Queue<Event>, io: &mut Queue<skein_io::Request>) {
        self.drive(env, up, io, MAX_OUT.above, MAX_OUT.below);
    }
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        let mut earliest = self.wake;
        for (_, run) in &self.runs {
            earlier(&mut earliest, run.deadline);
            if let Some(wait) = run.retry_at
                && wait < run.deadline
            {
                earlier(&mut earliest, wait);
            }
        }
        earliest
    }
    fn drive(
        &mut self,
        env: &Env<Limits>,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
        above: u32,
        below: u32,
    ) {
        let up_end = up.len().saturating_add(above);
        let io_end = io.len().saturating_add(below);
        self.wake = None;
        // Internal movements are bounded as well as boundary output (section 6.3).
        for _ in 0_u32..256 {
            if up.len() == up_end || io.len() == io_end {
                break;
            }
            let mut connection = None;
            for (token, c) in &self.connections {
                if c.ready() || c.result.is_some() || c.closed {
                    connection = Some(*token);
                    break;
                }
            }
            if let Some(token) = connection {
                self.drive_connection(token, env, io);
                continue;
            }
            let mut next = None;
            for (token, run) in &self.runs {
                if run.pending.is_some() || run.deadline <= env.now || self.can_send(run, env.now) {
                    next = Some(*token);
                    break;
                }
            }
            if let Some(token) = next {
                self.drive_run(token, env, up, io);
                continue;
            }
            if self.closing && self.connections.is_empty() && !self.ended {
                self.ended = true;
                up.push(Event::Closed);
            }
            return;
        }
        if self.ready(env.now) {
            self.wake = Some(env.now);
        }
    }
    fn can_send(&self, run: &Run, now: Time) -> bool {
        run.send.is_some()
            && run.connection.is_none()
            && match run.retry_at {
                Some(at) => at <= now,
                None => true,
            }
            && now < run.deadline
            && self.connections.len() < self.connections.capacity()
    }
    fn ready(&self, now: Time) -> bool {
        for (_, c) in &self.connections {
            if c.ready() || c.result.is_some() || c.closed {
                return true;
            }
        }
        for (_, run) in &self.runs {
            if run.pending.is_some() || run.deadline <= now || self.can_send(run, now) {
                return true;
            }
        }
        self.closing && !self.ended && self.connections.is_empty()
    }

    fn drive_connection(&mut self, token: Token, env: &Env<Limits>, io: &mut Queue<skein_io::Request>) {
        let c = self.connections.get_mut(&token).expect("owned connection");
        c.pump(env, io);
        if let Some(reply) = c.result.take()
            && let Some(run) = self.runs.get_mut(&c.run)
            && run.connection == Some(token)
        {
            run.reply(reply, &mut self.plan_out);
        }
        if c.closed && !c.ready() {
            let c = self.connections.remove(&token).expect("settled connection");
            if let Some(run) = self.runs.get_mut(&c.run)
                && run.connection == Some(token)
            {
                run.closed(env.now);
            }
        }
    }
    fn drive_run(&mut self, token: Token, env: &Env<Limits>, up: &mut Queue<Event>, io: &mut Queue<skein_io::Request>) {
        let run = self.runs.get_mut(&token).expect("live run");
        if let Some(need) = run.pending.take() {
            match need {
                reconcile::Need::Read(item) => {
                    let need = run.owner_need(Need::Read(item));
                    up.push(Event::Need { run: token, need });
                }
                reconcile::Need::Write { item, bytes } => {
                    let need = run.owner_need(Need::Write { item, bytes });
                    up.push(Event::Need { run: token, need });
                }
                reconcile::Need::Draw(count) => {
                    let mut drawn = bytes::zeroed(usize::try_from(count).expect("draw count fits"));
                    self.crypto.fill(&mut drawn);
                    run.step(reconcile::Event::Drawn(Secret::new(drawn)), &mut self.plan_out);
                }
                reconcile::Need::Send { request, token } => {
                    run.send = Some(Send { request, token });
                }
                reconcile::Need::Done(outcome) => {
                    up.push(Event::Done { run: token, outcome });
                    let _ended = self.runs.remove(&token);
                }
                reconcile::Need::Stop(failure) => {
                    up.push(Event::Stop { run: token, failure });
                    let _ended = self.runs.remove(&token);
                }
            }
            return;
        }
        if run.deadline <= env.now {
            if let Some(connection) = run.connection
                && let Some(c) = self.connections.get_mut(&connection)
            {
                c.abort();
            }
            if run.send.is_some() {
                run.step(reconcile::Event::Reply(Reply::Down), &mut self.plan_out);
            }
            run.halt(Why::Down, &mut self.plan_out);
            return;
        }
        let forge = configured(&self.settings, run.forge);
        let send = run.send.as_ref().expect("ready send");
        let Some(connection) =
            Connection::new(token, forge, &self.settings.user_agent, &send.request, &send.token, &env.limits)
        else {
            run.halt(Why::Unexpected, &mut self.plan_out);
            return;
        };
        let owner = Token::new(self.next_token);
        self.next_token = self.next_token.checked_add(1).expect("connection tokens never repeat");
        run.connection = Some(owner);
        run.retry_at = None;
        let previous = self.connections.insert(owner, connection).expect("connection room reserved");
        assert!(previous.is_none(), "fresh owner token");
        io.push(skein_io::Request::Connect { owner, addr: forge.address });
    }
}
fn earlier(earliest: &mut Option<Time>, time: Time) {
    if match *earliest {
        Some(before) => time < before,
        None => true,
    } {
        *earliest = Some(time);
    }
}
impl core::fmt::Debug for Host {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Host")
            .field("runs", &self.runs.len())
            .field("connections", &self.connections.len())
            .field("closing", &self.closing)
            .finish_non_exhaustive()
    }
}

fn configured(settings: &Settings, number: u16) -> &Forge {
    for forge in &settings.forges {
        if forge.given.forge == number {
            return forge;
        }
    }
    unreachable!("owner names a configured forge")
}
