//! Shared pending reply rights and scenario tables (fake-forge.md, 4).
use crate::{Config, Limits, Tally};
use alloc::boxed::Box;
use skein_fake_forge_domain as domain;
use skein_lib::{Id, List, Map, ReplyTo, Rng, Slab, Token};
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Error {
    Limits,
}
#[derive(Debug)]
pub(crate) struct Call {
    pub server: Token,
    pub active: bool,
}
#[derive(Debug)]
pub struct Service {
    pub(crate) config: Config,
    pub(crate) calls: Slab<Call>,
    pub(crate) rng: Rng,
    pub(crate) tally: Tally,
    pub(crate) setup: crate::setup::Store,
}
impl Service {
    pub fn new(config: Config, limits: &Limits, seed: u64) -> Result<Self, Error> {
        if let Some(endpoint) = &config.hooks
            && (endpoint.host.is_empty() || endpoint.secret.is_empty())
        {
            return Err(Error::Limits);
        }
        if worst_case(limits, &config).is_none()
            || config.max_response_items == 0
            || config.default_paging_num == 0
            || config.max_response_items > limits.documents.inner
            || config.base.len() > usize::try_from(limits.documents.target_bytes).expect("u32 fits")
            || config.faults.reset > 1000
            || config.faults.drop > 1000
            || config.faults.server_error > 1000
        {
            return Err(Error::Limits);
        }
        let authority = config.base.strip_prefix(b"http://").ok_or(Error::Limits)?;
        if authority.is_empty() {
            return Err(Error::Limits);
        }
        for byte in authority {
            if *byte == b'/' || !(0x21..=0x7e).contains(byte) {
                return Err(Error::Limits);
            }
        }
        if config.users.len() > limits.documents.inner
            || config.repositories.len() > limits.documents.inner
            || config.tokens.len() > limits.documents.inner
        {
            return Err(Error::Limits);
        }
        for (index, (token, user)) in config.tokens.iter().enumerate() {
            if token.is_empty() || token.len() > usize::try_from(limits.http.head).expect("u32 fits") || *user == 0 {
                return Err(Error::Limits);
            }
            for (prior, _) in config.tokens.iter().take(index) {
                if prior == token {
                    return Err(Error::Limits);
                }
            }
        }
        for (_, login) in &config.users {
            if login.len() > usize::try_from(limits.documents.string_bytes).expect("u32 fits") {
                return Err(Error::Limits);
            }
        }
        for (index, repo) in config.repositories.iter().enumerate() {
            if repo.id == 0 || repo.full.len() > usize::try_from(limits.documents.target_bytes).expect("u32 fits") {
                return Err(Error::Limits);
            }
            if let Some(branch) = &repo.protected
                && branch.len() > usize::try_from(limits.documents.string_bytes).expect("u32 fits")
            {
                return Err(Error::Limits);
            }
            for prior in config.repositories.iter().take(index) {
                if prior.id == repo.id || prior.full == repo.full {
                    return Err(Error::Limits);
                }
            }
        }
        let tally = Tally::new(config.tokens.len().saturating_add(limits.documents.inner));
        let setup = crate::setup::Store::new(limits);
        Ok(Self { config, calls: Slab::with_capacity(limits.calls), rng: Rng::new(seed), tally, setup })
    }
    #[expect(clippy::result_large_err, reason = "an unmatched owned reply returns unchanged with its reply right")]
    pub fn target(&mut self, request: domain::Request) -> Result<(Token, domain::Request), domain::Request> {
        match request {
            domain::Request::Reply { to, result } => {
                let token = to.into_token();
                let reply = domain::Request::Reply { to: ReplyTo::new(token), result };
                match self.calls.get(Id::<Call>::from_token(token)) {
                    Some(call) if call.active => Ok((call.server, reply)),
                    Some(_) | None => Err(reply),
                }
            }
            domain::Request::Hook { .. } => Err(request),
        }
    }
    #[must_use]
    pub const fn tally(&self) -> &Tally {
        &self.tally
    }
    pub fn reclaim(&mut self) {
        self.calls.reclaim();
    }
    pub(crate) fn user(&self, authorization: Option<&[u8]>) -> Option<u64> {
        let token = authorization?.strip_prefix(b"token ")?;
        for (known, user) in &self.config.tokens {
            if known.expose() == token {
                return Some(*user);
            }
        }
        self.setup.token_user(token)
    }
    pub(crate) fn reads_user(&self, authorization: Option<&[u8]>) -> bool {
        let Some(value) = authorization else {
            return false;
        };
        let Some(token) = value.strip_prefix(b"token ") else {
            return false;
        };
        for (known, _) in &self.config.tokens {
            if known.expose() == token {
                return true;
            }
        }
        self.setup.reads_user(token)
    }
    /// Scenario state for setup, without changing the repository domain.
    pub fn setup_mut(&mut self) -> &mut crate::setup::Store {
        &mut self.setup
    }
    #[must_use]
    pub fn setup(&self) -> &crate::setup::Store {
        &self.setup
    }
}
/// Tables and each connection's HTTP, JSON, request and encoded response ownership.
#[must_use]
pub fn worst_case(limits: &Limits, config: &Config) -> Option<u64> {
    if limits.calls == 0
        || limits.connections == 0
        || limits.raw_bytes == 0
        || skein_json::tokenizer::largest_demand(&limits.documents.json) > limits.http.read
        || skein_forge::forgejo::limits::check(&limits.documents).is_err()
    {
        return None;
    }
    let mut tables = u64::try_from(config.base.len()).ok()?;
    if let Some(endpoint) = &config.hooks {
        tables = tables
            .checked_add(u64::try_from(endpoint.host.len()).ok()?)?
            .checked_add(u64::try_from(endpoint.secret.len()).ok()?)?;
    }
    tables = tables
        .checked_add(List::<(skein_lib::Secret, u64)>::worst_case(config.tokens.capacity())?)?
        .checked_add(List::<(u64, Box<[u8]>)>::worst_case(config.users.capacity())?)?
        .checked_add(List::<crate::Repository>::worst_case(config.repositories.capacity())?)?;
    for (token, _) in &config.tokens {
        tables = tables.checked_add(u64::try_from(token.len()).ok()?)?;
    }
    for (_, login) in &config.users {
        tables = tables.checked_add(u64::try_from(login.len()).ok()?)?;
    }
    for repo in &config.repositories {
        tables = tables.checked_add(u64::try_from(repo.full.len()).ok()?)?;
        if let Some(branch) = &repo.protected {
            tables = tables.checked_add(u64::try_from(branch.len()).ok()?)?;
        }
    }
    let per_connection = skein_http::server::worst_case(&limits.http)?
        .checked_add(skein_json::tokenizer::worst_case(&limits.documents.json)?.checked_mul(2)?)?
        .checked_add(u64::from(limits.documents.body_bytes).checked_mul(4)?)?
        .checked_add(u64::from(limits.documents.writer.length).checked_mul(4)?)?
        .checked_add(
            u64::from(limits.documents.inner)
                .checked_mul(u64::from(limits.documents.string_bytes).checked_add(128)?)?,
        )?
        .checked_add(u64::from(limits.documents.target_bytes).checked_mul(2)?)?
        .checked_add(u64::from(limits.raw_bytes))?
        // One sanitized HTTP call held until its owner drains the entrance.
        .checked_add(u64::try_from(size_of::<Option<skein_http::server::Call>>()).ok()?)?
        .checked_add(u64::from(limits.http.head))?
        .checked_add(u64::from(limits.http.headers).checked_mul(u64::try_from(size_of::<skein_http::Header>()).ok()?)?)?
        .checked_add(skein_lib::Queue::<skein_http::server::Event>::worst_case(4)?)?
        .checked_add(skein_lib::Queue::<skein_http::server::Request>::worst_case(4)?)?
        .checked_add(skein_lib::Queue::<skein_json::tokenizer::Event>::worst_case(1)?)?
        .checked_add(skein_lib::Queue::<skein_lib::stream::Down>::worst_case(1)?)?;
    tables
        .checked_add(crate::setup::worst_case(limits)?)?
        .checked_add(Slab::<Call>::worst_case(limits.calls)?)?
        .checked_add(Map::<crate::Route, u64>::worst_case(crate::tally::ROUTES)?)?
        .checked_add(Map::<u64, u64>::worst_case(config.tokens.len().saturating_add(limits.documents.inner))?)?
        .checked_add(per_connection.checked_mul(u64::from(limits.connections))?)
}
