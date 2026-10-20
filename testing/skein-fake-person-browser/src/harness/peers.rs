//! Borrowed hosts share the loop's ring but retain their local tokens.
use super::Lent;
use skein_io::kernel::{Complete, Done, Op};
use skein_lib::{Queue, Time, Token, Wall};
use skein_world::ring::Ring;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub(super) const FIRST: usize = 2;
#[derive(Default)]
struct State {
    listening: Option<Token>,
    delivered: Option<Token>,
    address: Option<String>,
    stopped: bool,
    empty: bool,
    pending: bool,
    deadline: Option<Time>,
}
pub(super) struct Peers<'a, 'b> {
    lent: &'a mut [Lent<'b>],
    states: Vec<State>,
    pub limits: Vec<u32>,
}

/// Only a call into a lent host is caught; ring and harness faults propagate.
fn call<T>(name: &[u8], action: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(action)).map_err(|payload| {
        let text = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("non-text assertion");
        format!("lent peer {}: {text}", String::from_utf8_lossy(name))
    })
}
impl<'a, 'b> Peers<'a, 'b> {
    pub fn new(lent: &'a mut [Lent<'b>]) -> Result<Self, String> {
        let limits =
            lent.iter().map(|lent| call(lent.name, || lent.peer.operations())).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { states: (0..lent.len()).map(|_| State::default()).collect(), lent, limits })
    }
    pub fn ready(&self) -> bool {
        self.states.iter().all(|state| state.address.is_some())
    }
    pub fn addresses(&self) -> Vec<(Box<[u8]>, String)> {
        self.lent
            .iter()
            .zip(&self.states)
            .map(|(lent, state)| (lent.name.into(), state.address.clone().expect("listening")))
            .collect()
    }
    pub fn waiting(&self) -> String {
        self.lent
            .iter()
            .zip(&self.states)
            .filter(|(_, state)| state.address.is_none())
            .map(|(lent, _)| String::from_utf8_lossy(lent.name))
            .collect::<Vec<_>>()
            .join(", ")
    }
    pub fn completion(&mut self, host: usize, complete: &Complete) -> Option<String> {
        let index = host.checked_sub(FIRST).expect("peer host");
        let state = &mut self.states[index];
        if state.address.is_none() {
            if let Err(error) = &complete.result {
                return Some(format!(
                    "lent peer {} startup: {error:?}",
                    String::from_utf8_lossy(self.lent[index].name)
                ));
            }
            if let (Op::Listen { .. }, Ok(Done::Nothing)) = (&complete.kind, &complete.result) {
                state.listening = Some(complete.op);
            }
        }
        None
    }
    pub fn drive(&mut self, ring: &mut Ring, now: Time, wall: Wall, stop: bool) -> Vec<String> {
        let mut failures = Vec::new();
        for (index, (lent, state)) in self.lent.iter_mut().zip(&mut self.states).enumerate() {
            let host = FIRST + index;
            let room = match call(lent.name, || lent.peer.completions().room()) {
                Ok(room) => room,
                Err(error) => {
                    failures.push(error);
                    continue;
                }
            };
            let mut delivered = Queue::with_capacity(room);
            ring.deliver(host, &mut delivered);
            if let Some(token) = state.listening
                && delivered.iter().any(|complete| complete.op == token)
            {
                state.delivered = Some(token);
            }
            let result = call(lent.name, || {
                while let Some(complete) = delivered.pop() {
                    lent.peer.completions().push(complete);
                }
                if stop && !state.stopped {
                    lent.peer.shutdown();
                    state.stopped = true;
                }
                lent.peer.iterate(now, wall);
                lent.peer.drain();
                if state.address.is_none()
                    && state.delivered.is_some_and(|token| !lent.peer.completions().iter().any(|c| c.op == token))
                {
                    state.address = Some(lent.peer.address().to_string());
                }
                state.empty = lent.peer.is_empty();
                state.pending = lent.peer.work_pending(now);
                state.deadline = lent.peer.next_deadline();
                let mut submissions = Vec::new();
                while let Some(record) = lent.peer.submissions().pop() {
                    submissions.push(record);
                }
                submissions
            });
            match result {
                Ok(submissions) => {
                    for record in submissions {
                        let record = ring.register(host, record);
                        ring.queue(record);
                    }
                }
                Err(error) => failures.push(error),
            }
        }
        failures
    }
    pub fn empty(&self) -> bool {
        self.states.iter().all(|state| state.empty)
    }
    pub fn pending(&self) -> bool {
        self.states.iter().any(|state| state.pending)
    }
    pub fn deadline(&self) -> Option<Time> {
        self.states.iter().filter_map(|state| state.deadline).min()
    }
}
