//! TLS transport fixtures and byte faults for the scripted HTTP machine.
use crate::{
    fixtures,
    machine::{Connect, Cut, Machine},
};
use skein_forge::api::{Answer, Error};
use skein_forge_connection::{Settings, settings::Transport};
use skein_lib::Duration;
use skein_tls_world::pki;
const DEADLINE: Duration = Duration::from_secs(10);
const CALL: usize = 2;
/// The same admitted forge over the TLS world's trust override.
#[must_use]
pub fn settings() -> Settings {
    let mut settings = fixtures::settings();
    settings.forges.get_mut(0).expect("forge").transport =
        Transport::Tls { server_name: pki::name(), trust: pki::client(&[]) };
    settings
}
/// The complete byte sweep or the readers' state boundaries.
#[derive(Clone, Copy, Debug)]
pub enum Sweep {
    /// Every byte, in the exhaustive suite.
    Every,
    /// Each boundary and its adjacent bytes, in the fuzzy suite.
    Boundaries,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct HandshakeCut {
    pub response: bool,
    pub point: Position,
    pub reset: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Position {
    Byte(usize),
    Record { record: usize, header: bool, delta: i8 },
    End(i8),
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum CutStatus {
    At(usize),
    Duplicate,
    Missing,
}
impl Position {
    // All offsets, including deduplication, belong to this trial's flight.
    pub fn resolve(self, flight: &[u8]) -> CutStatus {
        if let Self::Byte(at) = self {
            return CutStatus::At(at.min(flight.len()));
        }
        let mut positions = Vec::new();
        let mut start = 0;
        let mut record = 0;
        while start < flight.len() {
            let header = flight.get(start..start + 5).expect("complete TLS record header");
            for (header, edge) in [(false, start), (true, start + 5)] {
                for delta in [-1, 0, 1] {
                    positions.push((Self::Record { record, header, delta }, adjacent(edge, delta, flight.len())));
                }
            }
            let length = usize::from(u16::from_be_bytes([header[3], header[4]]));
            start += 5 + length;
            assert!(start <= flight.len(), "whole captured TLS record");
            record += 1;
        }
        for delta in [-1, 0, 1] {
            positions.push((Self::End(delta), adjacent(flight.len(), delta, flight.len())));
        }
        let mut seen = std::collections::BTreeSet::new();
        for (position, offset) in positions {
            let unique = seen.insert(offset);
            if position == self {
                return if unique { CutStatus::At(offset) } else { CutStatus::Duplicate };
            }
        }
        CutStatus::Missing
    }
}
fn adjacent(edge: usize, delta: i8, length: usize) -> usize {
    edge.saturating_add_signed(isize::from(delta)).min(length)
}
/// EOF/reset across handshake flights and the plaintext HTTP framing they carry.
#[must_use]
pub fn tls_cuts(seed: u64, sweep: Sweep) -> Vec<(u64, u64, u64, u64)> {
    let mut control = machine(seed);
    control.call(1, fixtures::REPO, fixtures::locate());
    control.run_for(DEADLINE);
    let answer = control.answered(1).expect("TLS control");
    assert_eq!((answer.cost, &answer.result), (1, &Ok(Answer::Located { repository: 1 })));
    let requests = control.requests().to_vec();
    assert!(control.close());
    let mut rows = Vec::new();
    for response in [false, true] {
        for reset in [false, true] {
            match sweep {
                Sweep::Every => every_handshake(seed, response, reset, &mut rows),
                Sweep::Boundaries => handshake_boundaries(seed, response, reset, &mut rows),
            }
        }
    }
    for (request, bytes) in requests.into_iter().enumerate() {
        let response = match fixtures::locate_script().remove(request).response {
            crate::machine::Response::Bytes(bytes) => bytes,
            crate::machine::Response::Case(_)
            | crate::machine::Response::Reset
            | crate::machine::Response::Cut { .. }
            | crate::machine::Response::Silent => unreachable!("generated framing"),
        };
        for (response, bytes) in [(false, bytes), (true, response)] {
            for reset in [false, true] {
                for at in offsets(&bytes, sweep) {
                    let mut world = machine(seed);
                    world.cut(Cut { request, response, at, reset });
                    world.call(1, fixtures::REPO, fixtures::locate());
                    world.run_for(DEADLINE);
                    rows.push(outcome(&mut world, request == CALL && response && at == bytes.len()));
                }
            }
        }
    }
    rows
}
fn every_handshake(seed: u64, response: bool, reset: bool, rows: &mut Vec<(u64, u64, u64, u64)>) {
    let mut at = 0;
    loop {
        let mut world = handshake(seed, response, reset, Position::Byte(at));
        // The trial captures and cuts its own flight, including a variable
        // ECDSA signature; no other handshake supplies its length.
        let length = world.flight_length();
        rows.push(outcome(&mut world, false));
        if at >= length {
            break;
        }
        at += 1;
    }
}
fn handshake_boundaries(seed: u64, response: bool, reset: bool, rows: &mut Vec<(u64, u64, u64, u64)>) {
    'records: for record in 0.. {
        for header in [false, true] {
            for delta in [-1, 0, 1] {
                let point = Position::Record { record, header, delta };
                let mut world = handshake(seed, response, reset, point);
                match world.flight_status() {
                    CutStatus::At(_) => rows.push(outcome(&mut world, false)),
                    CutStatus::Duplicate => assert!(world.close()),
                    CutStatus::Missing => {
                        assert!(world.close());
                        break 'records;
                    }
                }
            }
        }
    }
    for delta in [-1, 0, 1] {
        let mut world = handshake(seed, response, reset, Position::End(delta));
        match world.flight_status() {
            CutStatus::At(_) => rows.push(outcome(&mut world, false)),
            CutStatus::Duplicate => assert!(world.close()),
            CutStatus::Missing => unreachable!("every captured flight has an end"),
        }
    }
}
fn handshake(seed: u64, response: bool, reset: bool, point: Position) -> Machine {
    let mut world = machine(seed);
    world.cut_handshake(HandshakeCut { response, point, reset });
    world.call(1, fixtures::REPO, fixtures::locate());
    world.run_for(DEADLINE);
    world
}
fn offsets(bytes: &[u8], sweep: Sweep) -> Vec<usize> {
    match sweep {
        Sweep::Every => (0..=bytes.len()).collect(),
        Sweep::Boundaries => {
            let mut edges = vec![0, bytes.len()];
            edges.extend(bytes.windows(2).enumerate().filter_map(|(at, pair)| (pair == b"\r\n").then_some(at + 2)));
            let mut offsets = edges
                .into_iter()
                .flat_map(|edge| [-1, 0, 1].map(|delta| adjacent(edge, delta, bytes.len())))
                .collect::<Vec<_>>();
            offsets.sort_unstable();
            offsets.dedup();
            offsets
        }
    }
}
fn machine(seed: u64) -> Machine {
    let mut world = Machine::with(seed, fixtures::limits(), settings(), fixtures::locate_script());
    let mut connects = [Connect::Refused; 16];
    connects[0] = Connect::Made;
    world.connects(&connects);
    world
}
fn outcome(world: &mut Machine, complete: bool) -> (u64, u64, u64, u64) {
    let written = u64::from(world.written_bytes().get(CALL).copied().unwrap_or(0) > 0);
    let answer = world.answered(1).expect("cut ends call");
    let expected = if complete {
        Ok(Answer::Located { repository: 1 })
    } else if written == 0 {
        Err(Error::Unavailable)
    } else {
        Err(Error::Timeout)
    };
    assert_eq!(answer.result, expected);
    let cost = u64::from(answer.cost);
    let terminals = u64::try_from(world.deliveries().len()).expect("small count");
    assert!(world.close());
    assert_eq!(world.event_order().last(), Some(&"closed"));
    (written, cost, terminals, u64::from(world.closed_events()))
}
