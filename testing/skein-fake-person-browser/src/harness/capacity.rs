//! Checked bounds for the harness's io; peers declare their own operations.
use super::Output;
use skein_browser as browser;
use skein_io as io;
use skein_lib::Duration;

// One in-flight exchange per person, plus its preceding closing socket.
const EXCHANGES_PER_PERSON: u32 = 2;
// Chromium's child and its command, reply and stderr pipes.
const BROWSER_SLOTS: u32 = 4;
// A command's child, stdin, stdout and stderr. Reserved before commands start.
const COMMAND_SLOTS: u32 = 4;
pub(super) const ROOT_OPERATIONS: u32 = 2;
pub(crate) const ORIGIN_CONNECTIONS: u32 = 16;

pub(super) struct Capacity {
    pub io: io::Limits,
    pub operations: u32,
    pub queue: u32,
    pub browser_queue: u32,
}
impl Capacity {
    pub fn new(people: u32, origins: u32, output: Output, bl: &browser::Limits) -> Option<Self> {
        let server_pipes = if output == Output::Stdout { 2_u32 } else { 1_u32 };
        let server = server_pipes.checked_add(1)?;
        let origins = origins.checked_mul(ORIGIN_CONNECTIONS.checked_add(1)?)?;
        let per_person = COMMAND_SLOTS.checked_add(EXCHANGES_PER_PERSON)?;
        let sockets =
            BROWSER_SLOTS.checked_add(server)?.checked_add(origins)?.checked_add(people.checked_mul(per_person)?)?;
        let io = io::Limits {
            sockets,
            refusals: sockets,
            intake: browser::largest_read(bl)?,
            receive: 65_536,
            output: browser::largest_room(bl)?,
            sends: 16,
            accepts: 4,
            backlog: 4,
            close_timeout: Duration::from_secs(2),
            names: 0,
            retry: Duration::from_millis(10),
        };
        let operations = io::operations(&io)?;
        // A completion may emit three events and two replacement submissions.
        // Entity resumptions and refused requests can coexist with completions.
        let queue = operations.checked_add(sockets)?.checked_mul(3)?.checked_add(io.refusals.checked_mul(2)?)?;
        let browser_queue =
            bl.ops.checked_add(bl.pages)?.checked_add(bl.persons)?.checked_add(bl.commands)?.checked_add(4)?;
        Some(Self { io, operations, queue, browser_queue })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_layout_fits_at_its_bound() {
        let capacity = Capacity::new(4, 2, Output::Stdout, &browser::Limits::default()).expect("bounded layout");
        // 4 browser + 3 server + 2 * (1 listener + 16 connections)
        // + 4 * (4 command + 2 exchange) = 65 slots, four ops each plus two.
        assert_eq!(capacity.io.sockets, 65);
        assert_eq!(capacity.operations, 262);
        assert_eq!(capacity.queue, 1111);
        assert_eq!(capacity.browser_queue, 216);
    }
    #[test]
    fn layout_refuses_arithmetic_overflow() {
        for (people, origins) in [(u32::MAX, 0), (0, u32::MAX)] {
            assert!(Capacity::new(people, origins, Output::Stdout, &browser::Limits::default()).is_none());
        }
    }
    fn admitted_layout(extra: bool) -> Vec<io::Event> {
        use skein_io::kernel::{Fd, Pipe, Spawn, Way};
        use skein_lib::{Env, Queue, Time, Token, Wall};
        let capacity = Capacity::new(4, 2, Output::Stdout, &browser::Limits::default()).expect("bounded layout");
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: capacity.io };
        let mut io = io::Io::new(&capacity.io);
        let mut submits = Queue::with_capacity(capacity.queue);
        let mut events = Queue::with_capacity(capacity.queue);
        // Chromium, the server, and four commands each reserve their child and pipes.
        for (index, pipes) in [3, 2, 3, 3, 3, 3].into_iter().enumerate() {
            io::down(
                &mut io,
                &env,
                io::Request::Spawn {
                    owner: Token::new(u64::try_from(index).expect("fixture index")),
                    spawn: Spawn {
                        program: b"fixture".as_slice().into(),
                        args: Box::new([]),
                        env: Box::new([]),
                        root: Fd::new(-1),
                        dir: b".".as_slice().into(),
                        pipes: (0..pipes).map(|child| Pipe { child, way: Way::Out, parent: None }).collect(),
                    },
                },
                &mut submits,
            );
        }
        // Two origin listeners and their 32 connections, plus eight exchanges.
        // Connect reserves the same single entity slot as an accepted socket.
        for index in 0..42_u64 + u64::from(extra) {
            let owner = Token::new(100 + index);
            let addr = io::Addr::Ip((std::net::Ipv4Addr::LOCALHOST, 0).into());
            let request =
                if index < 2 { io::Request::Listen { owner, addr } } else { io::Request::Connect { owner, addr } };
            io::down(&mut io, &env, request, &mut submits);
        }
        while io.is_ready() {
            io::resume(&mut io, &env, &mut events, &mut submits);
        }
        let mut result = Vec::new();
        while let Some(event) = events.pop() {
            result.push(event);
        }
        result
    }
    #[test]
    fn io_admits_every_entity_in_the_declared_layout() {
        let events = admitted_layout(false);
        assert!(!events.iter().any(|event| matches!(event, io::Event::Failed { .. })), "{events:?}");
    }
    #[test]
    fn io_refuses_one_entity_past_the_declared_layout() {
        let events = admitted_layout(true);
        assert!(
            events.iter().any(|event| matches!(event, io::Event::Failed { error: io::Error::Busy, .. })),
            "{events:?}"
        );
    }
}
