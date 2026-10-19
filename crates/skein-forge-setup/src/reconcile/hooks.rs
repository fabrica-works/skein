//! Listener hook convergence, retaining one lowest matching id
//! (protocol/setup.md, section 5, step 6).
use super::{
    Event, Need, Reconcile, Reply, Stage, State, Why,
    tokens::{reply, unexpected},
};
use crate::item::Item;
use alloc::boxed::Box;
use core::net::{IpAddr, SocketAddr};
use skein_forge::forgejo::{
    delivery::{SUBSCRIBED, hook_target},
    document::{Document, Row},
    request::{HookSpec, Page, Request},
};
use skein_lib::{Decimal, List, Secret, Writer};
/// A listener's URL, accepting the two loopback addresses only.
#[must_use]
pub fn hook_url(address: SocketAddr, forge: u16) -> Option<Box<[u8]>> {
    let host: Box<[u8]> = match address.ip() {
        IpAddr::V4(ip) if ip.is_loopback() => {
            let mut length = 3_usize;
            for octet in ip.octets() {
                length = length.saturating_add(Decimal::of(u64::from(octet)).as_bytes().len());
            }
            let mut host = Writer::new(length);
            for (index, octet) in ip.octets().iter().enumerate() {
                if index > 0 {
                    host.put(b".").expect("IPv4 dots");
                }
                host.put(Decimal::of(u64::from(*octet)).as_bytes()).expect("IPv4 octet");
            }
            host.finish()
        }
        IpAddr::V6(ip) if ip.is_loopback() => b"[::1]".as_slice().into(),
        IpAddr::V4(_) | IpAddr::V6(_) => return None,
    };
    let port = Decimal::of(u64::from(address.port()));
    let target = hook_target(forge);
    let mut out = Writer::new(
        8_usize.saturating_add(host.len()).saturating_add(port.as_bytes().len()).saturating_add(target.len()),
    );
    for part in [b"http://", host.as_ref(), b":", port.as_bytes(), &target] {
        out.put(part).expect("loopback URL fits");
    }
    Some(out.finish())
}
#[derive(Debug)]
enum Phase {
    Read,
    Draw,
    Written,
    Listing,
    Delete,
    Save,
}
#[derive(Debug)]
pub(super) struct Hooks {
    names: Box<[Box<[u8]>]>,
    at: usize,
    cap: u32,
    page: u32,
    url: Box<[u8]>,
    secret: Option<Secret>,
    kept: Option<u64>,
    phase: Phase,
}
impl Hooks {
    pub fn new(names: Box<[Box<[u8]>]>, cap: u32, url: Box<[u8]>) -> Self {
        Self { names, at: 0, cap, page: 1, url, secret: None, kept: None, phase: Phase::Read }
    }
    fn name(&self) -> &[u8] {
        self.names.get(self.at).expect("current org")
    }
    fn listing(&mut self, machine: &Reconcile) -> Need {
        self.phase = Phase::Listing;
        machine.send(Request::OrgHooks {
            org: self.name().into(),
            page: Page { page: Some(self.page), limit: Some(self.cap) },
        })
    }
    fn save(&mut self, machine: &Reconcile) -> Need {
        let mut events = List::with_capacity(u32::try_from(SUBSCRIBED.len()).expect("events fit"));
        for event in SUBSCRIBED {
            events.push(event.into()).expect("event capacity");
        }
        let hook = HookSpec {
            url: self.url.clone(),
            secret: self.secret.clone().expect("secret read"),
            events: events.into_boxed(),
            active: true,
        };
        self.phase = Phase::Save;
        machine.send(if let Some(id) = self.kept {
            Request::EditOrgHook { org: self.name().into(), id, hook }
        } else {
            Request::CreateOrgHook { org: self.name().into(), hook }
        })
    }
    fn event(&mut self, machine: &mut Reconcile, event: Event) -> Result<Need, Why> {
        match self.phase {
            Phase::Read => {
                if let Event::Read(value) = event {
                    if let Some(secret) = value {
                        self.secret = Some(secret);
                        Ok(self.listing(machine))
                    } else {
                        self.phase = Phase::Draw;
                        Ok(Need::Draw(32))
                    }
                } else {
                    Err(Why::Unexpected)
                }
            }
            Phase::Draw => {
                if let Event::Drawn(value) = event {
                    if value.len() != 32 {
                        return Err(Why::Unexpected);
                    }
                    let secret = super::hex(&value);
                    self.secret = Some(secret.clone());
                    self.phase = Phase::Written;
                    Ok(Need::Write { item: Item::WebhookSecret, bytes: secret })
                } else {
                    Err(Why::Unexpected)
                }
            }
            Phase::Written => {
                if event == Event::Written {
                    Ok(self.listing(machine))
                } else {
                    Err(Why::Unexpected)
                }
            }
            Phase::Listing => self.rows(machine, reply(event)?),
            Phase::Delete => {
                let answer = reply(event)?;
                if answer == Reply::Empty || answer == Reply::Status(404) {
                    Ok(self.listing(machine))
                } else {
                    Err(unexpected(answer))
                }
            }
            Phase::Save => {
                let answer = reply(event)?;
                if let Reply::Document(Document::Hook(_)) = answer {
                    self.at = self.at.checked_add(1).expect("bounded orgs");
                    if self.at == self.names.len() {
                        machine.state = State::Ended;
                        machine.admin = None;
                        Ok(Need::Done)
                    } else {
                        self.page = 1;
                        self.kept = None;
                        Ok(self.listing(machine))
                    }
                } else {
                    Err(unexpected(answer))
                }
            }
        }
    }
    fn rows(&mut self, machine: &Reconcile, answer: Reply) -> Result<Need, Why> {
        let Reply::Rows(rows) = answer else {
            return Err(unexpected(answer));
        };
        for row in &rows {
            let Row::Hook(hook) = row else {
                return Err(Why::Unexpected);
            };
            if hook.url != self.url {
                continue;
            }
            if let Some(kept) = self.kept {
                if kept == hook.id {
                    continue;
                }
                self.kept = Some(kept.min(hook.id));
                self.phase = Phase::Delete;
                return Ok(machine.send(Request::DeleteOrgHook { org: self.name().into(), id: kept.max(hook.id) }));
            }
            self.kept = Some(hook.id);
        }
        if rows.len() < usize::try_from(self.cap).expect("u32 fits") {
            Ok(self.save(machine))
        } else {
            self.page = self.page.checked_add(1).ok_or(Why::Unexpected)?;
            Ok(self.listing(machine))
        }
    }
}
pub(super) fn advance(machine: &mut Reconcile, event: Event) -> Need {
    machine.state = State::Hooks;
    let mut hooks = machine.hooks.take().expect("hook phase owns its walk");
    let need = match hooks.event(machine, event) {
        Ok(need) => need,
        Err(why) => machine.stop_named(Stage::Hook, hooks.name(), why),
    };
    machine.hooks = Some(hooks);
    need
}
