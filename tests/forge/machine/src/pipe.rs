//! Seeded fragments and resets, observing request framing and METHOD/target lines.
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
/// First-byte writes and requests ended before their delivered head completed.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestCounts {
    pub written: u64,
    pub cut_in_head: u64,
}

#[derive(Default, PartialEq, Eq)]
enum Head {
    #[default]
    Waiting,
    Line,
    Headers,
}

#[derive(Default)]
struct Framing {
    lines: Vec<Vec<u8>>,
    line: Vec<u8>,
    line_bytes: u64,
    length_line: bool,
    cr: bool,
    head: Head,
    body: u64,
    length: u64,
    written: u64,
    heads: u64,
}
impl Framing {
    fn bytes(&mut self, bytes: &[u8], retain_lines: bool) {
        for byte in bytes {
            if self.body > 0 {
                self.body -= 1;
                continue;
            }
            if self.head == Head::Waiting {
                self.written += 1;
                self.head = Head::Line;
                self.length_line = true;
                self.length = 0;
                if retain_lines {
                    self.lines.push(Vec::new());
                }
            }
            if self.head == Head::Line && retain_lines {
                self.lines.last_mut().expect("request line").push(*byte);
            }
            self.line_bytes += 1;
            if self.head == Head::Headers && self.length_line {
                let prefix = b"content-length:";
                if self.line.len() < prefix.len() && byte.to_ascii_lowercase() != prefix[self.line.len()] {
                    self.length_line = false;
                    self.line.clear();
                } else {
                    self.line.push(*byte);
                }
            }
            if self.cr && *byte == b'\n' {
                if self.head == Head::Line {
                    self.head = Head::Headers;
                } else if self.line_bytes == 2 {
                    self.heads += 1;
                    self.head = Head::Waiting;
                    self.body = self.length;
                } else if self.length_line {
                    self.length = std::str::from_utf8(&self.line[b"content-length:".len()..])
                        .expect("HTTP length")
                        .trim()
                        .parse()
                        .expect("HTTP length");
                }
                self.line.clear();
                self.line_bytes = 0;
                self.length_line = true;
            }
            self.cr = *byte == b'\r';
        }
    }
}
pub struct Pipe {
    client: End,
    server: End,
    rng: Rng,
    cut: Cut,
    carried: u64,
    sent: Framing,
    received: Framing,
    frozen: bool,
}
impl Pipe {
    #[must_use]
    pub fn new(seed: u64, cut: Cut) -> Self {
        Self {
            client: End::new(),
            server: End::new(),
            rng: Rng::new(seed),
            cut,
            carried: 0,
            sent: Framing::default(),
            received: Framing::default(),
            frozen: false,
        }
    }
    pub fn client_down(&mut self, down: Down) {
        if let Down::Send(bytes) = &down {
            self.sent.bytes(bytes, true);
        }
        self.client.down(&mut self.server, down);
    }
    pub fn server_down(&mut self, down: Down) {
        self.server.down(&mut self.client, down);
    }
    pub fn client_up(&mut self) -> Option<Up> {
        self.client.up(self.server.finished, &mut self.rng, &mut self.carried, self.cut)
    }
    pub fn server_up(&mut self) -> Option<Up> {
        let up = self.server.up(self.client.finished, &mut self.rng, &mut self.carried, self.cut);
        if let Some(Up::Bytes(bytes)) = &up {
            self.received.bytes(bytes, false);
        }
        up
    }
    pub(crate) fn freeze(&mut self) {
        self.frozen = true;
    }
    #[must_use]
    pub fn request_counts(&self) -> RequestCounts {
        let ended = self.frozen || self.client.failed || self.server.failed;
        RequestCounts {
            written: self.sent.written,
            cut_in_head: if ended {
                self.sent.written.checked_sub(self.received.heads).expect("heads follow writes")
            } else {
                0
            },
        }
    }
    #[must_use]
    pub fn request_lines(&self) -> &[Vec<u8>] {
        &self.sent.lines
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
