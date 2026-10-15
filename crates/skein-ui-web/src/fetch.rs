//! Bounded exchanges, identified independently of the application's ids.
use std::collections::BTreeMap;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[derive(Debug)]
pub struct Exchange {
    pub id: u32,
    pub path: Box<[u8]>,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Box<[u8]>,
    pub bound: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Report {
    Answered { exchange: u32, status: u16, body: Box<[u8]> },
    Failed { exchange: u32 },
}
struct Flight {
    id: u32,
    bound: u32,
    body: Vec<u8>,
}
pub struct Fetch {
    flights: BTreeMap<u64, Flight>,
    capacity: usize,
    serial: u64,
}
impl Fetch {
    #[must_use]
    pub fn new(capacity: u32) -> Self {
        Self { flights: BTreeMap::new(), capacity: usize::try_from(capacity).expect("capacity fits"), serial: 0 }
    }
    /// A ticket remains unique after the application abandons and reuses an id.
    pub fn begin(&mut self, id: u32, bound: u32) -> Option<u64> {
        if self.flights.len() == self.capacity || self.flights.values().any(|flight| flight.id == id) {
            return None;
        }
        self.serial = self.serial.checked_add(1).expect("exchange tickets fit");
        self.flights.insert(self.serial, Flight { id, bound, body: Vec::new() });
        Some(self.serial)
    }
    pub fn piece(&mut self, ticket: u64, piece: &[u8]) -> Option<Report> {
        let flight = self.flights.get_mut(&ticket)?;
        if piece.len() > usize::try_from(flight.bound).expect("bound fits").saturating_sub(flight.body.len()) {
            return self.fail(ticket);
        }
        flight.body.extend_from_slice(piece);
        None
    }
    #[must_use]
    pub fn remaining(&self, ticket: u64) -> Option<u32> {
        let flight = self.flights.get(&ticket)?;
        flight.bound.checked_sub(u32::try_from(flight.body.len()).expect("body within bound"))
    }
    pub fn finish(&mut self, ticket: u64, status: u16) -> Option<Report> {
        self.flights.remove(&ticket).map(|flight| Report::Answered {
            exchange: flight.id,
            status,
            body: flight.body.into_boxed_slice(),
        })
    }
    pub fn fail(&mut self, ticket: u64) -> Option<Report> {
        self.flights.remove(&ticket).map(|flight| Report::Failed { exchange: flight.id })
    }
    pub fn abandon(&mut self, id: u32) -> Option<u64> {
        let ticket = self.flights.iter().find_map(|(ticket, flight)| (flight.id == id).then_some(*ticket))?;
        self.flights.remove(&ticket);
        Some(ticket)
    }
}
