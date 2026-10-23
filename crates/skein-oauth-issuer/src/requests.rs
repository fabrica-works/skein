use crate::{
    Authorization, Error, Issuer, Refusal, TokenError, TokenRequest, documents,
    form::{self, Form},
    settings,
};
use alloc::boxed::Box;
use skein_lib::{
    Secret,
    base64::{self, Alphabet},
    bytes, sha256,
};

impl Issuer {
    pub fn authorize(&self, query: &[u8]) -> Result<Authorization, Refusal> {
        settings::bounded(query, self.limits.request_bytes).or(Err(Refusal::Malformed))?;
        let form = Form::read(query, Some(self.limits.state_bytes)).ok_or(Refusal::Malformed)?;
        if form.duplicate_binding {
            return Err(Refusal::Malformed);
        }
        let client =
            self.client(form.get(b"client_id").ok_or(Refusal::UnknownClient)?).ok_or(Refusal::UnknownClient)?;
        if form.get(b"redirect_uri") != Some(client.redirect_uri.as_ref()) {
            return Err(Refusal::UnknownRedirect);
        }
        let state = form.get(b"state");
        let long = form.past(b"state") || form.past(b"nonce");
        let response = form.get(b"response_type");
        let challenge = form.get(b"code_challenge").unwrap_or(b"");
        let mut valid_challenge = challenge.len() == 43;
        for &byte in challenge {
            if !form::url_character(byte) {
                valid_challenge = false;
            }
        }
        let error: Option<&[u8]> = if form.duplicate
            || response.is_none()
            || long
            || form.get(b"code_challenge_method") != Some(b"S256")
            || !valid_challenge
        {
            Some(b"invalid_request")
        } else if response != Some(b"code") {
            Some(b"unsupported_response_type")
        } else if !openid(form.get(b"scope").unwrap_or(b"")) {
            Some(b"invalid_scope")
        } else if form.get(b"request").is_some() {
            Some(b"request_not_supported")
        } else if form.get(b"request_uri").is_some() {
            Some(b"request_uri_not_supported")
        } else {
            None
        };
        if let Some(error) = error {
            let location =
                form::location(&client.redirect_uri, b"error", error, state).expect("bounded refusal location");
            return Err(Refusal::Back { location });
        }
        Ok(Authorization {
            client: client.id.clone(),
            redirect_uri: client.redirect_uri.clone(),
            state: copy_option(state),
            nonce: copy_option(form.get(b"nonce")),
            challenge: <[u8; 43]>::try_from(challenge).expect("challenge checked"),
        })
    }
    pub fn redirect(&self, authorization: &Authorization, code: &Secret) -> Result<Box<[u8]>, Error> {
        settings::bounded(code.expose(), self.limits.token_bytes)?;
        if code.is_empty() {
            return Err(Error::Limits);
        }
        for &byte in code.expose() {
            if !(0x20..=0x7e).contains(&byte) {
                return Err(Error::Limits);
            }
        }
        self.authorization(authorization)?;
        form::location(&authorization.redirect_uri, b"code", code.expose(), authorization.state.as_deref())
            .ok_or(Error::Limits)
    }
    pub(crate) fn authorization(&self, authorization: &Authorization) -> Result<(), Error> {
        settings::optional(authorization.state.as_deref(), self.limits.state_bytes)?;
        settings::optional(authorization.nonce.as_deref(), self.limits.state_bytes)?;
        let client = self.client(&authorization.client).ok_or(Error::Client)?;
        if client.redirect_uri != authorization.redirect_uri {
            return Err(Error::Client);
        }
        Ok(())
    }
    pub fn token_request(
        &self,
        authorization: Option<&[u8]>,
        content_type: Option<&[u8]>,
        body: &[u8],
    ) -> Result<TokenRequest, TokenError> {
        let content_type = content_type.ok_or(TokenError::InvalidRequest)?;
        let end = bytes::find(content_type, b";").unwrap_or(content_type.len());
        let media = content_type.get(..end).expect("media type prefix").trim_ascii();
        if !media.eq_ignore_ascii_case(b"application/x-www-form-urlencoded") {
            return Err(TokenError::InvalidRequest);
        }
        settings::bounded(body, self.limits.request_bytes).or(Err(TokenError::InvalidRequest))?;
        if let Some(header) = authorization {
            settings::bounded(header, self.limits.request_bytes).or(Err(TokenError::InvalidRequest))?;
        }
        let form = Form::read(body, None).ok_or(TokenError::InvalidRequest)?;
        if form.duplicate {
            return Err(TokenError::InvalidRequest);
        }
        let grant = form.get(b"grant_type").ok_or(TokenError::InvalidRequest)?;
        if grant != b"authorization_code" {
            return Err(TokenError::UnsupportedGrantType);
        }
        let header = authorization.is_some();
        let invalid_client = TokenError::InvalidClient { header };
        let (id, secret) = match authorization {
            Some(value) => {
                let (id, secret) = basic(value).ok_or(invalid_client)?;
                if form.get(b"client_secret").is_some() {
                    return Err(TokenError::InvalidRequest);
                }
                if let Some(body_id) = form.get(b"client_id")
                    && body_id != id.as_ref()
                {
                    return Err(TokenError::InvalidRequest);
                }
                (id, secret)
            }
            None => (
                bytes::copy_of(form.get(b"client_id").ok_or(invalid_client)?),
                Secret::new(bytes::copy_of(form.get(b"client_secret").ok_or(invalid_client)?)),
            ),
        };
        let client = self.client(&id).ok_or(invalid_client)?;
        if !sha256::equal(secret.expose(), client.secret.expose()) {
            return Err(invalid_client);
        }
        let code = form.get(b"code").ok_or(TokenError::InvalidRequest)?;
        let redirect_uri = form.get(b"redirect_uri").ok_or(TokenError::InvalidRequest)?;
        let verifier = form.get(b"code_verifier").ok_or(TokenError::InvalidRequest)?;
        if !(43..=128).contains(&verifier.len()) {
            return Err(TokenError::InvalidRequest);
        }
        for &byte in verifier {
            if !form::unreserved(byte) {
                return Err(TokenError::InvalidRequest);
            }
        }
        settings::bounded(code, self.limits.token_bytes).or(Err(TokenError::InvalidGrant))?;
        Ok(TokenRequest {
            client: id,
            code: Secret::new(bytes::copy_of(code)),
            redirect_uri: bytes::copy_of(redirect_uri),
            verifier: bytes::copy_of(verifier),
        })
    }
    pub fn redeem(&self, request: &TokenRequest, authorization: &Authorization) -> Result<(), TokenError> {
        if request.client != authorization.client || request.redirect_uri != authorization.redirect_uri {
            return Err(TokenError::InvalidGrant);
        }
        let digest = sha256::digest(&request.verifier);
        let challenge = documents::encoded(&digest).or(Err(TokenError::InvalidGrant))?;
        if !sha256::equal(&challenge, &authorization.challenge) {
            return Err(TokenError::InvalidGrant);
        }
        Ok(())
    }
    #[must_use]
    pub fn bearer(&self, authorization: Option<&[u8]>) -> Option<Secret> {
        let header = authorization?;
        settings::bounded(header, self.limits.request_bytes).ok()?;
        let (scheme, token) = scheme(header)?;
        if !scheme.eq_ignore_ascii_case(b"Bearer") || token.is_empty() || token.first() == Some(&b'=') {
            return None;
        }
        settings::bounded(token, self.limits.token_bytes).ok()?;
        let mut padding = false;
        for &byte in token {
            if byte == b'=' {
                padding = true;
            } else if padding || !(form::unreserved(byte) || byte == b'+' || byte == b'/') {
                return None;
            }
        }
        Some(Secret::new(bytes::copy_of(token)))
    }
}
fn basic(header: &[u8]) -> Option<(Box<[u8]>, Secret)> {
    let (scheme, encoded) = scheme(header)?;
    if !scheme.eq_ignore_ascii_case(b"Basic") {
        return None;
    }
    let raw = base64::decode(Alphabet::Standard, encoded, u32::try_from(header.len()).ok()?).ok()?;
    let at = bytes::find(&raw, b":")?;
    let id = form::decoded(raw.get(..at)?)?;
    let secret = form::decoded(raw.get(at.checked_add(1)?..)?)?;
    Some((id, Secret::new(secret)))
}
fn scheme(header: &[u8]) -> Option<(&[u8], &[u8])> {
    let at = bytes::find(header, b" ")?;
    Some((header.get(..at)?, header.get(at.checked_add(1)?..)?))
}
fn openid(scope: &[u8]) -> bool {
    let mut start = 0_usize;
    for end in 0..=scope.len() {
        if end == scope.len() || scope.get(end) == Some(&b' ') {
            if scope.get(start..end) == Some(b"openid") {
                return true;
            }
            start = end.checked_add(1).expect("bounded scope");
        }
    }
    false
}
#[expect(clippy::manual_map, reason = "step code uses exhaustive matches without closures")]
fn copy_option(value: Option<&[u8]>) -> Option<Box<[u8]>> {
    match value {
        Some(value) => Some(bytes::copy_of(value)),
        None => None,
    }
}
