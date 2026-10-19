//! A sign-in's bounded GETs, retries and move to its visit (oauth.md, section 3).
use super::{
    Client, ClientLimits, Exchange, Failure, HttpEvidence, HttpRequest, HttpResponse, Method, Phase, Registration,
    Request,
};
use super::{Grant, SignInValues, admit};
use crate::redirect::{authorization_url, has_openid, sign_in_values, validate_registration};
use crate::{Check, keys, pkce};
use alloc::boxed::Box;
use skein_lib::{Duration, Queue, Time, Writer};

pub(super) enum Fetch {
    Metadata { endpoint: Box<[u8]> },
    Keys { endpoint: Box<[u8]> },
}
pub(super) fn failure() -> Failure {
    Failure::Unverified { check: Check::Keys }
}

pub(super) fn initial(registration: &Registration, limits: &ClientLimits) -> Result<Fetch, Failure> {
    let openid = registration.terms.as_ref().ok_or(failure())?.openid.as_ref().ok_or(failure())?;
    if let Some(endpoint) = &openid.key_set {
        return Ok(Fetch::Keys { endpoint: endpoint.clone() });
    }
    let suffix = b"/.well-known/openid-configuration";
    let size = openid.issuer.len().checked_add(suffix.len()).ok_or(failure())?;
    if size > usize::try_from(limits.uri_bytes).expect("u32 fits") {
        return Err(failure());
    }
    let mut out = Writer::new(size);
    out.put(&openid.issuer).expect("measured metadata URL");
    out.put(suffix).expect("measured metadata URL");
    Ok(Fetch::Metadata { endpoint: out.finish() })
}

impl Client {
    pub(super) fn sign_in(
        &mut self,
        mut registration: Registration,
        values: SignInValues,
    ) -> Result<(Exchange, Request, Phase), Failure> {
        let SignInValues { key, generation, state, verifier, nonce, port, issued_client_id, id_token, subject, now } =
            values;
        validate_registration(&registration, &self.limits, true)?;
        sign_in_values(&mut registration, nonce.as_deref(), port, &self.limits)?;
        if state.len() < 16 || state.len() > usize::try_from(self.limits.state_bytes).expect("u32 fits") {
            return Err(Failure::Limit);
        }
        let pkce_required = registration.client_secret.is_none() || registration.pkce_for_confidential;
        let challenge = match (&verifier, pkce_required) {
            (Some(value), true) => Some(match pkce::challenge(value) {
                Ok(challenge) => challenge,
                Err(error) => return Err(admit(error)),
            }),
            (None, false) => None,
            (Some(_), false) | (None, true) => return Err(Failure::Malformed),
        };
        let url = authorization_url(
            &registration,
            &state,
            challenge.as_deref(),
            nonce.as_deref(),
            [issued_client_id.as_deref(), id_token.as_deref(), subject.as_deref()],
            &self.limits,
        )?;
        let deadline = now.checked_add(self.limits.sign_in_time).ok_or(Failure::Limit)?;
        let mut exchange = Exchange {
            registration,
            grant: Grant::SignIn { key, generation, code: None },
            state,
            verifier,
            deadline,
            attempts: 0,
            visit: None,
            fetch: None,
            keys: Box::default(),
        };
        if has_openid(&exchange.registration) {
            exchange.visit = Some(url);
            exchange.fetch = Some(initial(&exchange.registration, &self.limits)?);
            let (request, phase) = self.http_request(&mut exchange, now)?;
            Ok((exchange, Request::Http(request), phase))
        } else {
            Ok((exchange, Request::Visit { url }, Phase::AwaitRedirect))
        }
    }

    pub(super) fn keys_request(
        &mut self,
        exchange: &mut Exchange,
        _now: Time,
        deadline: Time,
    ) -> Result<(HttpRequest, Phase), Failure> {
        let endpoint = match exchange.fetch.as_ref().ok_or(failure())? {
            Fetch::Metadata { endpoint } | Fetch::Keys { endpoint } => endpoint.clone(),
        };
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(failure())?;
        exchange.attempts = exchange.attempts.checked_add(1).ok_or(failure())?;
        Ok((
            HttpRequest { id, method: Method::Get, endpoint, content_type: b"", body: Box::default(), deadline },
            Phase::AwaitKeys { id, deadline },
        ))
    }

    pub(super) fn keys_answer(&mut self, answer: HttpResponse, deadline: Time, out: &mut Queue<Request>) {
        let Some(mut exchange) = self.active.take() else {
            self.finish(failure(), out);
            return;
        };
        if answer.now >= exchange.deadline {
            self.finish(failure(), out);
            return;
        }
        if answer.now >= deadline
            || answer.evidence != HttpEvidence::Response
            || answer.status == 429
            || answer.status >= 500
        {
            self.retry_or_fail(exchange, Failure::Unavailable, answer.now, answer.retry_after, true, out);
            return;
        }
        if !(200..300).contains(&answer.status) {
            self.finish(failure(), out);
            return;
        }
        let result = self.read_keys(&mut exchange, &answer.body, answer.now);
        match result {
            Ok((request, phase)) => {
                self.active = Some(exchange);
                self.phase = phase;
                out.push(request);
            }
            Err(_) => self.finish(failure(), out),
        }
    }

    fn read_keys(&mut self, exchange: &mut Exchange, body: &[u8], now: Time) -> Result<(Request, Phase), Check> {
        let fetch = exchange.fetch.take().ok_or(Check::Keys)?;
        match fetch {
            Fetch::Metadata { .. } => {
                let issuer = &exchange
                    .registration
                    .terms
                    .as_ref()
                    .ok_or(Check::Keys)?
                    .openid
                    .as_ref()
                    .ok_or(Check::Keys)?
                    .issuer;
                let endpoint = keys::issuer_key_set(body, issuer, &self.limits)?;
                exchange.fetch = Some(Fetch::Keys { endpoint });
                exchange.attempts = 0;
                let (request, phase) = self.http_request(exchange, now).or(Err(Check::Keys))?;
                Ok((Request::Http(request), phase))
            }
            Fetch::Keys { .. } => {
                exchange.keys = keys::fitting_keys(body, &self.limits)?;
                exchange.attempts = 0;
                let url = exchange.visit.take().ok_or(Check::Keys)?;
                Ok((Request::Visit { url }, Phase::AwaitRedirect))
            }
        }
    }

    pub(super) fn keys_timeout(&mut self, now: Time, out: &mut Queue<Request>) {
        match self.active.take() {
            Some(exchange) => self.retry_or_fail(exchange, Failure::TimedOut, now, Duration::ZERO, true, out),
            None => self.finish(failure(), out),
        }
    }
}
