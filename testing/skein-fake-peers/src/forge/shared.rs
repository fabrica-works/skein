//! Shared pending ownership and checked hosted heap accounting.
use super::{Call, Connection, Forge, Observation, call_capacity};
use crate::{Limits, Transport, face, transport};
use skein_fake_forge_domain as domain;
use skein_fake_forge_protocol as protocol;
use skein_lib::{Queue, Time, Token};
use std::mem::size_of;

const EXTERNAL: u64 = 7 << 60;
impl Forge {
    #[must_use]
    pub fn new(domain: domain::Domain, config: domain::Config) -> Self {
        let capacity = config.limits.calls.checked_add(config.limits.hooks).expect("bounded pending capacity");
        Self { domain, config, now: Time::ZERO, pending: Queue::with_capacity(capacity), hooks: None, serial: 0 }
    }
    /// External reply rights use slots outside the HTTP service's admitted range.
    pub fn next_reply(&mut self) -> Token {
        self.serial =
            self.serial.checked_add(1).filter(|serial| *serial < (1 << 60)).expect("finite external reply rights");
        Token::new(EXTERNAL.checked_add(self.serial).expect("disjoint external reply range"))
    }
}

pub(super) fn bound(limits: &Limits, protocol: &protocol::Limits, protocol_bound: u64, forge: &Forge) -> Option<u64> {
    let config = &forge.config;
    let domain = &config.limits;
    if limits.io.sockets < limits.connections.checked_mul(2)?.checked_add(1)?
        || limits.connections > protocol.connections
        || protocol.calls >= (7 << 28)
        || limits.plaintext < protocol.http.head.max(protocol.http.read)
        || limits.io.output < protocol.http.response.max(protocol.http.send)
        || forge.pending.capacity() < domain.calls.checked_add(domain.hooks)?
        || domain.repositories == 0
        || domain.users == 0
        || domain.items == 0
        || domain.commits == 0
        || domain.files == 0
        || domain.branches == 0
        || domain.page_size == 0
        || domain.calls == 0
        || config.latency_min > config.latency_max
        || config.late_min > config.late_max
        || config.land_min > config.land_max
        || config.hook_min > config.hook_max
        || (config.rate_limit > 0 && config.rate_window == skein_lib::Duration::ZERO)
        || [config.late, config.unavailable, config.timeouts, config.landing, config.hooks_late, config.hooks_lost]
            .into_iter()
            .any(|chance| chance > 1000)
    {
        return None;
    }
    let domain_bound = domain::worst_case(domain)?;
    // The domain envelope contains its largest possible answer and hook payload.
    // Protocol ownership also bounds rejected call inputs, before domain validation.
    let payload = domain_bound.checked_add(protocol_bound)?;
    let connection = transport::worst_case(limits, Transport::Plaintext)?
        .checked_add(u64::try_from(size_of::<Connection>()).ok()?)?
        .checked_add(Queue::<protocol::Event>::worst_case(limits.queue)?)?
        .checked_add(Queue::<skein_lib::stream::Down>::worst_case(limits.queue)?)?
        .checked_add(Queue::<skein_lib::stream::Up>::worst_case(limits.queue)?)?
        .checked_add(u64::from(limits.queue).checked_mul(payload.checked_add(u64::from(limits.plaintext))?)?)?;
    face::worst_case(limits)?
        .checked_add(super::delivery::worst_case(limits, protocol, forge.hooks.as_ref())?)?
        .checked_add(protocol_bound)?
        .checked_add(domain_bound)?
        .checked_add(Queue::<domain::Request>::worst_case(forge.pending.capacity())?)?
        .checked_add(u64::from(forge.pending.capacity()).checked_mul(domain_bound)?)?
        .checked_add(u64::from(limits.connections).checked_mul(connection)?)?
        .checked_add(
            u64::from(call_capacity(protocol, forge)?)
                .checked_mul(u64::try_from(size_of::<Call>()).ok()?.checked_add(payload)?)?,
        )?
        .checked_add(u64::from(limits.observations).checked_mul(u64::try_from(size_of::<Observation>()).ok()?)?)?
        .checked_add(u64::from(limits.observation_bytes))
}
