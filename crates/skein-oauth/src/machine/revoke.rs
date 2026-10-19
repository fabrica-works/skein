//! Refresh-token revocation and its confirmed or unconfirmed terminal.

use super::{Client, ClientLimits, Exchange, Failure, Grant, HttpEvidence, HttpResponse, Phase, Registration, Request};
use crate::RefreshState;
use crate::redirect::{form_fields, issued_id, validate_registration};
use alloc::boxed::Box;
use skein_lib::{Queue, Time, bytes};

pub(super) fn endpoint(registration: &Registration) -> Option<&[u8]> {
    registration.terms.as_ref()?.revocation_endpoint.as_deref()
}

impl Exchange {
    pub(super) fn revoking(&self) -> bool {
        match self.grant {
            Grant::Revoke { .. } => true,
            Grant::SignIn { .. } | Grant::Refresh { .. } => false,
        }
    }
}

impl Client {
    pub(super) fn revoke(
        &mut self,
        registration: Registration,
        prior: RefreshState,
        now: Time,
        out: &mut Queue<Request>,
    ) {
        let result = self.start_revocation(registration, prior, now);
        match result {
            Ok(start) => self.emit_start(Ok(start), out),
            Err(_) => self.revoked(false, out),
        }
    }

    fn start_revocation(
        &mut self,
        mut registration: Registration,
        prior: RefreshState,
        now: Time,
    ) -> Result<(Exchange, Request, Phase), Failure> {
        validate_registration(&registration, &self.limits, false)?;
        endpoint(&registration).ok_or(Failure::Malformed)?;
        if prior.refresh_token.is_empty()
            || prior.refresh_token.len() > usize::try_from(self.limits.document.token_bytes).expect("u32 fits")
        {
            return Err(Failure::Limit);
        }
        if let Err(error) = crate::record::validate_refresh_additions(&prior, &self.limits.document) {
            return Err(super::admit(error));
        }
        if issued_id(&registration) {
            let Some(client_id) = &prior.issued_client_id else { return Err(Failure::Refused) };
            if client_id.is_empty() {
                return Err(Failure::Refused);
            }
            registration.client_id.clone_from(client_id);
        }
        let mut exchange = Exchange {
            registration,
            grant: Grant::Revoke { prior },
            state: bytes::copy_of(b""),
            verifier: None,
            nonce: None,
            deadline: now.checked_add(self.limits.sign_in_time).ok_or(Failure::Limit)?,
            attempts: 0,
            visit: None,
            fetch: None,
            keys: Box::default(),
        };
        let (request, phase) = self.http_request(&mut exchange, now)?;
        Ok((exchange, Request::Http(request), phase))
    }

    pub(super) fn revocation_answer(
        &mut self,
        exchange: Exchange,
        answer: HttpResponse,
        deadline: Time,
        out: &mut Queue<Request>,
    ) {
        if answer.now >= deadline || answer.now >= exchange.deadline {
            self.revoked(false, out);
        } else if answer.evidence == HttpEvidence::Unsent {
            self.retry_or_fail(exchange, Failure::Unavailable, answer.now, answer.retry_after, true, out);
        } else if answer.status == 429 && answer.evidence == HttpEvidence::Response {
            self.retry_or_fail(
                exchange,
                Failure::RateLimited { retry_after: answer.retry_after },
                answer.now,
                answer.retry_after,
                true,
                out,
            );
        } else {
            self.revoked(answer.status == 200 && answer.evidence == HttpEvidence::Response, out);
        }
    }

    pub(super) fn revoked(&mut self, confirmed: bool, out: &mut Queue<Request>) {
        self.active = None;
        self.phase = Phase::Done;
        out.push(Request::Revoked { confirmed });
    }
}

pub(super) fn body(
    registration: &Registration,
    prior: &RefreshState,
    limits: &ClientLimits,
) -> Result<Box<[u8]>, Failure> {
    let fields = [
        Some((b"token".as_slice(), prior.refresh_token.as_ref())),
        Some((b"token_type_hint".as_slice(), b"refresh_token".as_slice())),
        Some((b"client_id".as_slice(), registration.client_id.as_ref())),
        match registration.client_secret.as_deref() {
            Some(secret) => Some((b"client_secret".as_slice(), secret)),
            None => None,
        },
    ];
    form_fields(&fields, limits.request_bytes)
}
