//! Seeded byte fragments and resets, without recording bytes or credentials.
use skein_lib::stream::{Down, Fault, Read, Up};
use skein_lib::{Intake, Rng};
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug)]
pub enum Cut {
    Never,
    At(u64),
}
struct End {
    intake: Intake,
    pending: VecDeque<u8>,
    demand: Option<(Read, u32)>,
    granted: Option<u32>,
    finished: bool,
    ended: bool,
    failed: bool,
}
impl End {
    fn new() -> Self {
        Self {
            intake: Intake::with_capacity(65536),
            pending: VecDeque::new(),
            demand: None,
            granted: None,
            finished: false,
            ended: false,
            failed: false,
        }
    }
    fn down(&mut self, other: &mut End, down: Down) {
        match down {
            Down::Demand { read: Read::Nothing, room: 0 } => {
                assert!(self.demand.take().is_some(), "withdraw a demand");
            }
            Down::Demand { read, room } => {
                assert!(self.demand.replace((read, room)).is_none(), "one demand");
                if room > 0 {
                    assert!(!self.finished, "room before Finish");
                    self.granted = None;
                }
            }
            Down::Send(bytes) => {
                let room = self.granted.take().expect("granted room");
                assert!(bytes.len() <= usize::try_from(room).expect("room"), "send fits");
                other.pending.extend(bytes);
            }
            Down::Finish => {
                assert!(!self.finished, "one Finish");
                self.finished = true;
            }
        }
    }
    fn up(&mut self, eof: bool, rng: &mut Rng, carried: &mut u64, cut: Cut) -> Option<Up> {
        if matches!(cut,Cut::At(at) if *carried >= at) {
            if self.failed {
                return None;
            }
            self.failed = true;
            self.demand = None;
            return Some(Up::Failed(Fault::Reset));
        }
        if !self.pending.is_empty() && self.intake.room() > 0 && rng.chance(700) {
            let mut length = usize::try_from(rng.between(1, 127))
                .expect("fragment")
                .min(self.pending.len())
                .min(usize::try_from(self.intake.room()).expect("room"));
            if let Cut::At(at) = cut {
                length = length.min(usize::try_from(at.saturating_sub(*carried)).unwrap_or(usize::MAX));
            }
            let bytes: Vec<_> = self.pending.drain(..length).collect();
            self.intake.append(&bytes).expect("intake room");
            *carried += u64::try_from(length).expect("fragment");
        }
        let (read, room) = self.demand?;
        if let Some(bytes) = self.intake.meet(read) {
            self.demand = None;
            Some(Up::Bytes(bytes))
        } else if room > 0 && rng.chance(700) {
            self.demand = None;
            self.granted = Some(room);
            Some(Up::Room)
        } else if eof && self.pending.is_empty() && read != Read::Nothing && !self.ended {
            self.ended = true;
            Some(Up::End)
        } else {
            None
        }
    }
    fn busy(&self, eof: bool) -> bool {
        !self.pending.is_empty()
            || self.demand.is_some_and(|(read, _)| read != Read::Nothing && !self.intake.is_empty())
            || self.demand.is_some_and(|(read, room)| room > 0 || (read != Read::Nothing && eof && !self.ended))
    }
}
pub struct Pipe {
    client: End,
    server: End,
    rng: Rng,
    cut: Cut,
    carried: u64,
}
impl Pipe {
    #[must_use]
    pub fn new(seed: u64, cut: Cut) -> Self {
        Self { client: End::new(), server: End::new(), rng: Rng::new(seed), cut, carried: 0 }
    }
    pub fn client_down(&mut self, down: Down) {
        self.client.down(&mut self.server, down);
    }
    pub fn server_down(&mut self, down: Down) {
        self.server.down(&mut self.client, down);
    }
    pub fn client_up(&mut self) -> Option<Up> {
        self.client.up(self.server.finished, &mut self.rng, &mut self.carried, self.cut)
    }
    pub fn server_up(&mut self) -> Option<Up> {
        self.server.up(self.client.finished, &mut self.rng, &mut self.carried, self.cut)
    }
    #[must_use]
    pub fn carried(&self) -> u64 {
        self.carried
    }
    #[must_use]
    pub fn has_work(&self) -> bool {
        self.client.busy(self.server.finished)
            || self.server.busy(self.client.finished)
            || matches!(self.cut,Cut::At(at) if self.carried>=at && (!self.client.failed || !self.server.failed))
    }
}
