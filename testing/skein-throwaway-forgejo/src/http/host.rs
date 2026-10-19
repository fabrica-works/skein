//! A single admitted socket, closed at the HTTP terminal and fully drained.
use super::ExchangeHost;
use crate::Failure;
use skein_io::{self as io, kernel};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_shell::{Clock, Host, Kernel};

fn limits() -> io::Limits {
    io::Limits {
        sockets: 1,
        names: 0,
        refusals: 1,
        intake: 65_536,
        receive: 16_384,
        output: 65_536,
        sends: 4,
        accepts: 1,
        backlog: 1,
        close_timeout: Duration::from_millis(100),
        retry: Duration::from_millis(1),
    }
}
struct OneShot<'a> {
    io: io::Io,
    complete: Queue<kernel::Complete>,
    submit: Queue<kernel::Submit>,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
    socket: Option<Token>,
    exchange: Option<&'a mut ExchangeHost>,
    deadline: Time,
    closing: bool,
    connected: bool,
}
impl OneShot<'_> {
    fn new(exchange: Option<&mut ExchangeHost>, deadline: Time) -> OneShot<'_> {
        OneShot {
            io: io::Io::new(&limits()),
            complete: Queue::with_capacity(64),
            submit: Queue::with_capacity(64),
            events: Queue::with_capacity(64),
            requests: Queue::with_capacity(64),
            socket: None,
            exchange,
            deadline,
            closing: false,
            connected: false,
        }
    }
    fn stop(&mut self) {
        if !self.closing {
            self.closing = true;
            if let Some(socket) = self.socket {
                self.requests.push(io::Request::Abort { entity: socket });
            }
        }
    }
}
impl Host for OneShot<'_> {
    fn iterate(&mut self, now: Time, wall: Wall) {
        let env = Env { now, wall, limits: limits() };
        for _ in 0..64 {
            if self.events.room() < 3 || self.submit.room() < 2 {
                break;
            }
            if self.io.is_ready() {
                io::resume(&mut self.io, &env, &mut self.events, &mut self.submit);
            } else if let Some(complete) = self.complete.pop() {
                io::up(&mut self.io, &env, complete, &mut self.events, &mut self.submit);
            } else if self.io.is_due(now) {
                io::fire(&mut self.io, &env, &mut self.events, &mut self.submit);
            } else {
                break;
            }
        }
        while let Some(event) = self.events.pop() {
            match event {
                io::Event::Connecting { socket, .. } => self.socket = Some(socket),
                io::Event::Connected { .. } => {
                    self.connected = true;
                    self.deadline = now.saturating_add(Duration::from_secs(10));
                }
                io::Event::Stream { up, .. } => {
                    if !self.closing
                        && let Some(exchange) = self.exchange.as_mut()
                    {
                        exchange.up(up);
                    }
                }
                io::Event::Failed { error, .. } => {
                    if let Some(exchange) = self.exchange.as_mut() {
                        exchange.fail(format!("connection failed: {error:?}"));
                    }
                    self.stop();
                }
                io::Event::Closed { .. } => {
                    self.socket = None;
                }
                io::Event::Listening { .. }
                | io::Event::Accepted { .. }
                | io::Event::Output { .. }
                | io::Event::Spawned { .. }
                | io::Event::Exited { .. }
                | io::Event::Usage { .. }
                | io::Event::Resized
                | io::Event::Window { .. }
                | io::Event::Shutdown { .. } => unreachable!("one-shot client socket events"),
            }
        }
        if !self.closing && now >= self.deadline {
            if let Some(exchange) = self.exchange.as_mut() {
                exchange.fail("HTTP exchange timed out".into());
            }
            self.stop();
        }
        if self.exchange.as_ref().is_some_and(|exchange| exchange.done) {
            self.stop();
        }
        if !self.closing
            && self.connected
            && let (Some(socket), Some(exchange)) = (self.socket, self.exchange.as_mut())
        {
            while self.requests.room() > 0 {
                let Some(down) = exchange.below.pop() else {
                    break;
                };
                exchange.capture(&down);
                self.requests.push(io::Request::Stream { stream: socket, down });
            }
        }
        while self.io.takes() && self.submit.room() >= 2 {
            let Some(request) = self.requests.pop() else {
                break;
            };
            io::down(&mut self.io, &env, request, &mut self.submit);
        }
        self.io.reclaim();
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.complete
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submit
    }
    fn work_pending(&self, _now: Time) -> bool {
        self.io.is_ready() || !self.complete.is_empty() || !self.events.is_empty() || !self.requests.is_empty()
    }
    fn next_deadline(&self) -> Option<Time> {
        [self.io.next_deadline(), if self.closing { None } else { Some(self.deadline) }].into_iter().flatten().min()
    }
    fn next_policy_deadline(&self) -> Option<Time> {
        if self.closing { None } else { Some(self.deadline) }
    }
    fn is_empty(&self) -> bool {
        self.closing
            && self.io.is_empty()
            && self.complete.is_empty()
            && self.submit.is_empty()
            && self.requests.is_empty()
            && self.events.is_empty()
    }
    fn worst_case(&self) -> u64 {
        io::worst_case(&limits()).expect("one socket limits")
    }
    fn operations(&self) -> u32 {
        io::operations(&limits()).expect("one socket operations")
    }
}
fn drive(mut host: OneShot<'_>, clock: &Clock) -> Result<(), Failure> {
    let mut kernel = Kernel::open(skein_shell::Config { operations: host.operations() })
        .map_err(|error| Failure::Io(format!("HTTP ring: {error:?}")))?;
    skein_shell::drive(&mut kernel, clock, &mut host);
    Ok(())
}
pub(super) fn exchange(port: u16, exchange: &mut ExchangeHost) -> Result<(), Failure> {
    let clock = Clock::new();
    let mut host = OneShot::new(Some(exchange), clock.now().now.saturating_add(Duration::from_millis(100)));
    host.requests
        .push(io::Request::Connect { owner: Token::new(0), addr: io::Addr::Ip(([127, 0, 0, 1], port).into()) });
    drive(host, &clock)
}
pub(crate) fn wait(duration: Duration) -> Result<(), Failure> {
    let clock = Clock::new();
    drive(OneShot::new(None, clock.now().now.saturating_add(duration)), &clock)
}
