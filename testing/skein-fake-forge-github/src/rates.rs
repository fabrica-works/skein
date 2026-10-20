//! One token admission per HTTP request; continuations retain its rate snapshot.
use crate::{
    answers,
    auth::Bearer,
    routes,
    service::{Outcome, Request, Service},
};
use alloc::boxed::Box;
use skein_fake_forge_domain as domain;
use skein_lib::{Duration, Env, ReplyTo, Wall, bytes};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Heads {
    pub rate: domain::Rate,
    pub after: Option<Duration>,
    pub at: Wall,
}
#[derive(Debug)]
pub(crate) enum Route {
    Token(routes::Token),
    App,
}
/// Retains bounded route and body facts, with neither a credential nor a reply right.
pub struct Admission {
    route: Route,
    bearer: Bearer,
    target: Box<[u8]>,
    body: Box<[u8]>,
    method: skein_forge::github::request::Method,
    now: Wall,
}
impl core::fmt::Debug for Admission {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Admission")
            .field("route", &self.route)
            .field("bearer", &self.bearer)
            .field("target_bytes", &self.target.len())
            .field("body_bytes", &self.body.len())
            .field("method", &self.method)
            .field("now", &self.now)
            .finish()
    }
}
impl Admission {
    pub(crate) fn new(route: Route, bearer: Bearer, request: &Request<'_>, now: Wall, body_bound: u32) -> Self {
        // One excess byte preserves the oversize decision without retaining arbitrary input.
        let cap = usize::try_from(body_bound).expect("u32 fits").saturating_add(1).min(request.body.len());
        Self {
            route,
            bearer,
            target: bytes::copy_of(request.target),
            body: bytes::copy_of(request.body.get(..cap).expect("bounded prefix")),
            method: request.method,
            now,
        }
    }
    #[must_use]
    pub const fn key(&self) -> domain::RateKey {
        match self.bearer {
            Bearer::Installation { installation, .. } => domain::RateKey::Installation(installation),
            Bearer::Person { user, .. } => domain::RateKey::User(user),
        }
    }
}
impl Service {
    /// Charges the authenticated identity in the shared domain's bounded windows.
    pub fn admit(
        &mut self,
        admission: Admission,
        domain: &mut domain::Domain,
        env: &Env<domain::Config>,
        reply_to: ReplyTo,
    ) -> Outcome {
        let result = domain::rate(domain, env, admission.key(), self.rate_quota, self.rate_window);
        self.admitted(admission, result, reply_to)
    }
    /// Completes the host's domain-window admission without retaining the domain.
    pub fn admitted(
        &mut self,
        admission: Admission,
        result: Result<domain::Rate, domain::RateDenied>,
        reply_to: ReplyTo,
    ) -> Outcome {
        let (outcome, heads) = match result {
            Ok(rate) => {
                let request = Request {
                    validator: None,
                    method: admission.method,
                    target: &admission.target,
                    body: &admission.body,
                    authorization: None,
                    version: None,
                    agent: true,
                };
                let outcome = match admission.route {
                    Route::Token(route) => self.token_route(route, admission.now, admission.bearer, &request, reply_to),
                    Route::App => Outcome::Reply(answers::unauthorized(&self.limits)),
                };
                (outcome, Heads { rate, after: None, at: admission.now })
            }
            Err(domain::RateDenied::Limited(rate)) => (
                Outcome::Reply(answers::refusal(403, crate::texts::PRIMARY_RATE, &self.limits)),
                Heads { rate, after: None, at: admission.now },
            ),
            Err(domain::RateDenied::Crowded { rate, after }) => (
                Outcome::Reply(answers::refusal(403, crate::texts::SECONDARY_RATE, &self.limits)),
                Heads { rate, after: Some(after), at: admission.now },
            ),
        };
        attach(self.local_links(outcome), Some(heads))
    }
    #[must_use]
    pub const fn rate_quota(&self) -> u32 {
        self.rate_quota
    }
    #[must_use]
    pub const fn rate_window(&self) -> Duration {
        self.rate_window
    }
}
pub(crate) fn attach(outcome: Outcome, rate: Option<Heads>) -> Outcome {
    match outcome {
        Outcome::Reply(mut response) => {
            response.rate = rate;
            Outcome::Reply(response)
        }
        Outcome::Call(mut pending, event) => {
            pending.rate = rate;
            Outcome::Call(pending, event)
        }
        Outcome::Admit(admission) => Outcome::Admit(admission),
    }
}

fn decimal(value: u64) -> Box<[u8]> {
    bytes::copy_of(skein_lib::Decimal::of(value).as_bytes())
}
/// Appends the five core rate fields and at most one Retry-After; reserve six slots.
pub fn headers(heads: Heads, refused: bool, headers: &mut skein_lib::List<skein_http::Header>) {
    for (name, value) in [
        (b"X-RateLimit-Limit".as_slice(), decimal(u64::from(heads.rate.limit))),
        (b"X-RateLimit-Remaining", decimal(u64::from(heads.rate.limit.saturating_sub(heads.rate.used)))),
        (b"X-RateLimit-Used", decimal(u64::from(heads.rate.used))),
        (
            b"X-RateLimit-Reset",
            decimal(heads.rate.reset.as_nanos().checked_div(1_000_000_000).expect("nonzero seconds")),
        ),
        (b"X-RateLimit-Resource", bytes::copy_of(b"core")),
    ] {
        headers.push(skein_http::Header { name: bytes::copy_of(name), value }).expect("rate header slot");
    }
    // The primary refusal also supplies Retry-After, so the client need not
    // derive a delay from the world clock when no Date was emitted.
    let after = match heads.after {
        Some(after) => Some(after),
        None if refused && heads.rate.used >= heads.rate.limit => {
            Some(Duration::from_nanos(heads.rate.reset.as_nanos().saturating_sub(heads.at.as_nanos())))
        }
        None => None,
    };
    if let Some(after) = after {
        let seconds = after.as_nanos().div_ceil(1_000_000_000);
        headers
            .push(skein_http::Header { name: bytes::copy_of(b"Retry-After"), value: decimal(seconds) })
            .expect("retry header slot");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixtures as f;
    use skein_forge::github::{pin, request::Method};
    use skein_lib::{List, Secret, Token};

    #[test]
    fn github_fake_admission_heap_contains_only_request_facts_independent_of_token_length() {
        let target = b"/installation/token";
        for length in [1, 32, 256] {
            let secret = Secret::new(f::boxed([b'!'; 256].get(..length).expect("bounded token length")));
            let mut config = f::config();
            config.tokens = List::with_capacity(1);
            config.tokens.push((secret.clone(), 31)).unwrap();
            let mut h = f::Harness::with(f::limits(), config);
            let authorization = f::bearer(secret.expose());
            let Outcome::Admit(admission) = h.service.request(
                &Request {
                    method: Method::Delete,
                    target,
                    body: b"",
                    authorization: Some(authorization.expose()),
                    version: Some(pin::API_VERSION.as_bytes()),
                    agent: true,
                    validator: None,
                },
                h.now,
                ReplyTo::new(Token::new(77)),
            ) else {
                panic!("token admission")
            };
            assert_eq!(admission.key(), domain::RateKey::User(31));
            assert_eq!(admission.target.len() + admission.body.len(), target.len());
            for bytes in [&admission.target, &admission.body] {
                assert!(bytes::find(bytes, secret.expose()).is_none());
            }
            // The handle has no heap; the target and body exhaust the owned bytes.
            let Bearer::Person { index: 0, user: 31 } = admission.bearer else { panic!("person handle") };
        }
    }
}
