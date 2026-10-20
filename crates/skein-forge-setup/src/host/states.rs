//! Bounded one-time authority for setup callbacks (forge-setup.md, 5.7).
use crate::reconcile::{Failure, Stage, Why};
use alloc::boxed::Box;
use skein_lib::{Duration, Queue, Time};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Purpose {
    Convert,
    Accept,
}
impl Purpose {
    fn name(self) -> &'static [u8] {
        match self {
            Self::Convert => b"convert",
            Self::Accept => b"accept",
        }
    }
}
#[derive(Debug)]
struct Entry {
    state: Box<[u8]>,
    forge: u16,
    purpose: Purpose,
    birth: Time,
    expiry: Time,
}
#[derive(Debug)]
pub(super) struct States {
    entries: Queue<Entry>,
}
impl States {
    pub(super) fn new(capacity: u32) -> Self {
        assert!(capacity > 0, "host limits admit a nonzero state table");
        Self { entries: Queue::with_capacity(capacity) }
    }
    /// Issues host-drawn bytes as lowercase hex, bound to forge and purpose (5.7).
    /// Expired entries go first, then the oldest issuance if full. A collision
    /// or expiry overflow refuses without evicting a live binding.
    pub(super) fn issue(
        &mut self,
        forge: u16,
        purpose: Purpose,
        random: &[u8; 16],
        now: Time,
        life: Duration,
        subject: &[u8],
    ) -> Result<Box<[u8]>, Failure> {
        self.prune(now);
        let failure = Failure { forge, step: Stage::State, subject: subject.into(), why: Why::Unexpected };
        let Some(expiry) = now.checked_add(life) else {
            return Err(failure);
        };
        let mut text = [0_u8; 32];
        for (pair, byte) in text.chunks_exact_mut(2).zip(random) {
            *pair.first_mut().expect("two hex digits") = hex(byte.wrapping_shr(4));
            *pair.get_mut(1).expect("two hex digits") = hex(byte & 0x0f);
        }
        for entry in &self.entries {
            if entry.state.as_ref() == text {
                return Err(failure);
            }
        }
        if self.entries.room() == 0 {
            let _oldest = self.entries.pop().expect("a full nonzero table");
        }
        self.entries.push(Entry { state: text.into(), forge, purpose, birth: now, expiry });
        Ok(text.into())
    }
    /// Takes a matching live binding once, valid on [birth, expiry) (5.7).
    /// A wrong forge or purpose leaves the right binding available.
    pub(super) fn take(&mut self, forge: u16, purpose: Purpose, state: &[u8], now: Time) -> Result<(), Failure> {
        self.prune(now);
        let mut taken = false;
        for _ in 0..self.entries.len() {
            let entry = self.entries.pop().expect("bounded traversal");
            let same = skein_lib::sha256::equal(&entry.state, state);
            if entry.forge == forge && entry.purpose == purpose && same && entry.birth <= now {
                taken = true;
            } else {
                self.entries.push(entry);
            }
        }
        if taken {
            Ok(())
        } else {
            Err(Failure { forge, step: Stage::State, subject: purpose.name().into(), why: Why::Unknown })
        }
    }
    /// Close forgets every binding without arming an expiry timer (5.7).
    pub(super) fn clear(&mut self) {
        for _ in 0..self.entries.len() {
            let _entry = self.entries.pop().expect("bounded traversal");
        }
    }
    fn prune(&mut self, now: Time) {
        for _ in 0..self.entries.len() {
            let entry = self.entries.pop().expect("bounded traversal");
            if now < entry.expiry {
                self.entries.push(entry);
            }
        }
    }
}
fn hex(value: u8) -> u8 {
    match value {
        0..=9 => b'0'.checked_add(value).expect("decimal digit"),
        10..=15 => b'a'.checked_add(value.checked_sub(10).expect("hex letter")).expect("hex digit"),
        16..=255 => unreachable!("a nibble is below sixteen"),
    }
}
