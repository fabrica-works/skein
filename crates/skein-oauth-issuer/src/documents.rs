//! Measured JSON, JWK thumbprints and compact RS256 ID tokens.
use crate::{Answer, Authorization, Error, Issuer, Person, TokenError, form, settings};
use alloc::boxed::Box;
use skein_json::writer::{Encoder, Limits as JsonLimits};
use skein_lib::{
    Duration, Secret, Wall, Writer,
    base64::{self, Alphabet},
    bytes,
};

const JSON: JsonLimits = JsonLimits { depth: 4, length: u32::MAX };

impl Issuer {
    #[must_use]
    pub fn discovery(&self) -> Box<[u8]> {
        let mut measure = Encoder::measure(&JSON);
        self.write_discovery(&mut measure);
        let len = measure.measured().expect("new admitted discovery");
        let mut out = Encoder::write(len, &JSON);
        self.write_discovery(&mut out);
        out.finish()
    }
    fn write_discovery(&self, out: &mut Encoder) {
        out.object_start();
        for (name, value) in [
            (b"issuer".as_slice(), &self.settings.issuer),
            (b"authorization_endpoint", &self.settings.authorization_endpoint),
            (b"token_endpoint", &self.settings.token_endpoint),
            (b"userinfo_endpoint", &self.settings.userinfo_endpoint),
            (b"jwks_uri", &self.settings.jwks_uri),
        ] {
            text(out, name, value);
        }
        for (name, values) in [
            (b"response_types_supported".as_slice(), [b"code".as_slice()].as_slice()),
            (b"response_modes_supported", [b"query".as_slice()].as_slice()),
            (b"grant_types_supported", [b"authorization_code".as_slice()].as_slice()),
            (b"subject_types_supported", [b"public".as_slice()].as_slice()),
            (b"id_token_signing_alg_values_supported", [b"RS256".as_slice()].as_slice()),
            (b"scopes_supported", [b"openid".as_slice(), b"profile", b"email"].as_slice()),
            (
                b"token_endpoint_auth_methods_supported",
                [b"client_secret_basic".as_slice(), b"client_secret_post"].as_slice(),
            ),
            (b"code_challenge_methods_supported", [b"S256".as_slice()].as_slice()),
            (
                b"claims_supported",
                [
                    b"sub".as_slice(),
                    b"iss",
                    b"aud",
                    b"exp",
                    b"iat",
                    b"nonce",
                    b"preferred_username",
                    b"email",
                    b"email_verified",
                    b"name",
                ]
                .as_slice(),
            ),
        ] {
            out.key(name);
            out.array_start();
            for value in values {
                out.string(value);
            }
            out.array_end();
        }
        out.object_end();
    }
    #[must_use]
    pub fn key_set(&self) -> Box<[u8]> {
        let mut measure = Encoder::measure(&JSON);
        self.write_keys(&mut measure);
        let len = measure.measured().expect("new admitted key set");
        let mut out = Encoder::write(len, &JSON);
        self.write_keys(&mut out);
        out.finish()
    }
    fn write_keys(&self, out: &mut Encoder) {
        out.object_start();
        out.key(b"keys");
        out.array_start();
        out.object_start();
        for (name, value) in [
            (b"kty".as_slice(), b"RSA".as_slice()),
            (b"use", b"sig"),
            (b"alg", b"RS256"),
            (b"kid", &self.kid),
            (b"n", &self.modulus),
            (b"e", &self.exponent),
        ] {
            text(out, name, value);
        }
        out.object_end();
        out.array_end();
        out.object_end();
    }
    pub fn tokens(
        &self,
        authorization: &Authorization,
        person: &Person,
        access: &Secret,
        life: Duration,
        wall: Wall,
    ) -> Result<Box<[u8]>, Error> {
        self.authorization(authorization)?;
        self.person(person)?;
        settings::bounded(access.expose(), self.limits.token_bytes)?;
        if access.is_empty() {
            return Err(Error::Limits);
        }
        for &byte in access.expose() {
            if !form::url_character(byte) {
                return Err(Error::Limits);
            }
        }
        if life.as_nanos() == 0 || !life.as_nanos().is_multiple_of(1_000_000_000) {
            return Err(Error::Limits);
        }
        let seconds = life.as_nanos().checked_div(1_000_000_000).expect("nonzero divisor");
        let expires = wall.as_secs().checked_add(seconds).ok_or(Error::Limits)?;
        let mut measure = Encoder::measure(&JSON);
        write_header(&mut measure, &self.kid);
        let len = measure.measured().or(Err(Error::Limits))?;
        let mut out = Encoder::write(len, &JSON);
        write_header(&mut out, &self.kid);
        let header = encoded(&out.finish())?;
        let mut measure = Encoder::measure(&JSON);
        self.write_claims(&mut measure, authorization, person, expires, wall);
        let len = measure.measured().or(Err(Error::Limits))?;
        let mut out = Encoder::write(len, &JSON);
        self.write_claims(&mut out, authorization, person, expires, wall);
        let payload = encoded(&out.finish())?;
        let input = joined(&header, &payload)?;
        let signature = skein_tls::sign_rs256_pkcs8(self.settings.key.expose(), &input).ok_or(Error::Sign)?;
        let jwt = joined(&input, &encoded(&signature)?)?;
        let mut measure = Encoder::measure(&JSON);
        write_answer(&mut measure, access, seconds, &jwt);
        let len = measure.measured().or(Err(Error::Limits))?;
        let mut out = Encoder::write(len, &JSON);
        write_answer(&mut out, access, seconds, &jwt);
        Ok(out.finish())
    }
    fn write_claims(
        &self,
        out: &mut Encoder,
        authorization: &Authorization,
        person: &Person,
        expires: u64,
        wall: Wall,
    ) {
        out.object_start();
        text(out, b"iss", &self.settings.issuer);
        text(out, b"sub", &person.subject);
        text(out, b"aud", &authorization.client);
        out.key(b"exp");
        out.unsigned(expires);
        out.key(b"iat");
        out.unsigned(wall.as_secs());
        if let Some(nonce) = &authorization.nonce {
            text(out, b"nonce", nonce);
        }
        person_claims(out, person);
        out.object_end();
    }
    pub fn userinfo(&self, person: &Person) -> Result<Box<[u8]>, Error> {
        self.person(person)?;
        let mut measure = Encoder::measure(&JSON);
        write_userinfo(&mut measure, person);
        let len = measure.measured().or(Err(Error::Limits))?;
        let mut out = Encoder::write(len, &JSON);
        write_userinfo(&mut out, person);
        Ok(out.finish())
    }
    #[expect(clippy::disallowed_methods, reason = "the issuer protocol encoder validates each bounded claim as UTF-8")]
    fn person(&self, person: &Person) -> Result<(), Error> {
        if person.subject.is_empty() {
            return Err(Error::Claims);
        }
        for value in [Some(person.subject.as_ref()), person.username.as_deref(), person.name.as_deref(), email(person)]
            .into_iter()
            .flatten()
        {
            if settings::bounded(value, self.limits.claim_bytes).is_err() || core::str::from_utf8(value).is_err() {
                return Err(Error::Claims);
            }
        }
        Ok(())
    }
    #[must_use]
    pub fn userinfo_refused(&self) -> Answer {
        Answer { status: 401, challenge: Some(bytes::copy_of(b"Bearer error=\"invalid_token\"")), body: Box::default() }
    }
}
fn email(person: &Person) -> Option<&[u8]> {
    match &person.email {
        Some(email) => Some(&email.address),
        None => None,
    }
}
fn write_header(out: &mut Encoder, kid: &[u8]) {
    out.object_start();
    text(out, b"alg", b"RS256");
    text(out, b"typ", b"JWT");
    text(out, b"kid", kid);
    out.object_end();
}
fn write_answer(out: &mut Encoder, access: &Secret, seconds: u64, jwt: &[u8]) {
    out.object_start();
    text(out, b"access_token", access.expose());
    text(out, b"token_type", b"Bearer");
    out.key(b"expires_in");
    out.unsigned(seconds);
    text(out, b"id_token", jwt);
    out.object_end();
}
fn write_userinfo(out: &mut Encoder, person: &Person) {
    out.object_start();
    text(out, b"sub", &person.subject);
    person_claims(out, person);
    out.object_end();
}
fn person_claims(out: &mut Encoder, person: &Person) {
    if let Some(username) = &person.username {
        text(out, b"preferred_username", username);
    }
    if let Some(email) = &person.email {
        text(out, b"email", &email.address);
        out.key(b"email_verified");
        out.boolean(email.verified);
    }
    if let Some(name) = &person.name {
        text(out, b"name", name);
    }
}
fn text(out: &mut Encoder, name: &[u8], value: &[u8]) {
    out.key(name);
    out.string(value);
}
pub(crate) fn thumbprint(modulus: &[u8], exponent: &[u8]) -> Result<Box<[u8]>, Error> {
    let mut measure = Encoder::measure(&JSON);
    write_thumbprint(&mut measure, modulus, exponent);
    let len = measure.measured().or(Err(Error::Limits))?;
    let mut out = Encoder::write(len, &JSON);
    write_thumbprint(&mut out, modulus, exponent);
    Ok(out.finish())
}
fn write_thumbprint(out: &mut Encoder, modulus: &[u8], exponent: &[u8]) {
    out.object_start();
    text(out, b"e", exponent);
    text(out, b"kty", b"RSA");
    text(out, b"n", modulus);
    out.object_end();
}
pub(crate) fn encoded(input: &[u8]) -> Result<Box<[u8]>, Error> {
    let len =
        base64::encoded_len(Alphabet::Url, u32::try_from(input.len()).or(Err(Error::Limits))?).ok_or(Error::Limits)?;
    let mut out = Writer::new(usize::try_from(len).expect("u32 fits usize"));
    base64::encode(Alphabet::Url, input, &mut out).or(Err(Error::Limits))?;
    Ok(out.finish())
}
fn joined(left: &[u8], right: &[u8]) -> Result<Box<[u8]>, Error> {
    let len = left.len().checked_add(right.len()).ok_or(Error::Limits)?.checked_add(1).ok_or(Error::Limits)?;
    let mut out = Writer::new(len);
    out.put(left).expect("measured left part");
    out.put(b".").expect("measured separator");
    out.put(right).expect("measured right part");
    Ok(out.finish())
}
impl TokenError {
    #[must_use]
    pub fn answer(self) -> Answer {
        let (status, challenge, name) = match self {
            TokenError::InvalidRequest => (400, None, b"invalid_request".as_slice()),
            TokenError::InvalidClient { header } => {
                (401, if header { Some(bytes::copy_of(b"Basic")) } else { None }, b"invalid_client".as_slice())
            }
            TokenError::InvalidGrant => (400, None, b"invalid_grant".as_slice()),
            TokenError::UnsupportedGrantType => (400, None, b"unsupported_grant_type".as_slice()),
        };
        let mut measure = Encoder::measure(&JSON);
        write_error(&mut measure, name);
        let len = measure.measured().expect("fixed token error");
        let mut out = Encoder::write(len, &JSON);
        write_error(&mut out, name);
        Answer { status, challenge, body: out.finish() }
    }
}
fn write_error(out: &mut Encoder, name: &[u8]) {
    out.object_start();
    text(out, b"error", name);
    out.object_end();
}
