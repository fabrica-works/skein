use super::*;
const TYPE: Option<&[u8]> = Some(b"application/x-www-form-urlencoded");
pub(super) fn basic(raw: &[u8]) -> Box<[u8]> {
    let mut out = skein_lib::Writer::new(
        usize::try_from(
            skein_lib::base64::encoded_len(
                skein_lib::base64::Alphabet::Standard,
                u32::try_from(raw.len()).expect("length"),
            )
            .expect("length"),
        )
        .expect("usize"),
    );
    skein_lib::base64::encode(skein_lib::base64::Alphabet::Standard, raw, &mut out).expect("base64");
    [b"Basic ".as_slice(), &out.finish()].concat().into_boxed_slice()
}
#[test]
fn token_request_refusals_follow_the_contract_order() {
    let issuer = issuer();
    let valid = basic(b"client:secret");
    let no_body_secret = replace(BODY, "&client_secret=secret", "");
    for (label, header, content_type, body, expected) in [
        ("no content type", None, None, boxed(BODY), TokenError::InvalidRequest),
        ("JSON type", None, Some(b"application/json".as_slice()), boxed(BODY), TokenError::InvalidRequest),
        ("escape before type", None, TYPE, boxed(b"grant_type=other&x=%xx"), TokenError::InvalidRequest),
        ("duplicate", None, TYPE, replace(BODY, "code=code", "code=code&code=other"), TokenError::InvalidRequest),
        ("unknown duplicate", None, TYPE, replace(BODY, "code=code", "code=code&x=1&x=2"), TokenError::InvalidRequest),
        ("no grant", None, TYPE, boxed(b""), TokenError::InvalidRequest),
        ("empty grant", None, TYPE, boxed(b"grant_type="), TokenError::InvalidRequest),
        ("unsupported before auth", None, TYPE, boxed(b"grant_type=refresh_token"), TokenError::UnsupportedGrantType),
        (
            "other scheme",
            Some(b"Bearer x".as_slice()),
            TYPE,
            no_body_secret.clone(),
            TokenError::InvalidClient { header: true },
        ),
        ("bad base64", Some(b"Basic !!"), TYPE, no_body_secret.clone(), TokenError::InvalidClient { header: true }),
        (
            "extra space",
            Some(b"Basic  Y2xpZW50OnNlY3JldA=="),
            TYPE,
            no_body_secret.clone(),
            TokenError::InvalidClient { header: true },
        ),
        (
            "missing padding",
            Some(b"Basic Y2xpZW50OnNlY3JldA"),
            TYPE,
            no_body_secret.clone(),
            TokenError::InvalidClient { header: true },
        ),
        ("no colon", Some(b"Basic Y2xpZW50"), TYPE, no_body_secret.clone(), TokenError::InvalidClient { header: true }),
        (
            "bad percent in Basic",
            Some(b"Basic JXh4OnNlY3JldA=="),
            TYPE,
            no_body_secret.clone(),
            TokenError::InvalidClient { header: true },
        ),
        ("both methods", Some(valid.as_ref()), TYPE, boxed(BODY), TokenError::InvalidRequest),
        (
            "body id conflicts",
            Some(valid.as_ref()),
            TYPE,
            replace(&no_body_secret, "client_id=client", "client_id=other"),
            TokenError::InvalidRequest,
        ),
        (
            "body id missing",
            None,
            TYPE,
            replace(BODY, "client_id=client", "client_id="),
            TokenError::InvalidClient { header: false },
        ),
        ("body secret missing", None, TYPE, no_body_secret.clone(), TokenError::InvalidClient { header: false }),
        (
            "body id unknown",
            None,
            TYPE,
            replace(BODY, "client_id=client", "client_id=other"),
            TokenError::InvalidClient { header: false },
        ),
        (
            "secret one byte wrong",
            None,
            TYPE,
            replace(BODY, "secret=secret", "secret=secreu"),
            TokenError::InvalidClient { header: false },
        ),
        ("no code", None, TYPE, replace(BODY, "code=code", "code="), TokenError::InvalidRequest),
        ("no redirect", None, TYPE, replace(BODY, "redirect_uri=", "x="), TokenError::InvalidRequest),
        ("no verifier", None, TYPE, replace(BODY, "code_verifier=", "x="), TokenError::InvalidRequest),
        (
            "short verifier",
            None,
            TYPE,
            replace(BODY, "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk", "x"),
            TokenError::InvalidRequest,
        ),
        ("verifier alphabet", None, TYPE, replace(BODY, "dBjft", "%2BBjft"), TokenError::InvalidRequest),
    ] {
        assert_eq!(issuer.token_request(header, content_type, &body).expect_err(label), expected, "{label}");
    }
    assert_eq!(
        issuer.token_request(Some(&basic(b"client:secreu")), TYPE, &no_body_secret).expect_err("header wrong secret"),
        TokenError::InvalidClient { header: true }
    );
}
#[test]
fn both_auth_methods_and_a_form_encoded_basic_id_are_accepted() {
    let issuer = issuer();
    let request =
        issuer.token_request(None, Some(b"Application/X-WWW-Form-Urlencoded;charset=UTF-8"), BODY).expect("post");
    assert_eq!(request.code.expose(), b"code");
    issuer.redeem(&request, &issuer.authorize(QUERY).expect("auth")).expect("RFC 7636 B");
    let header = basic(b"client:secret");
    let body = replace(BODY, "&client_secret=secret", "");
    assert_eq!(issuer.token_request(Some(&header), TYPE, &body).expect("Basic").client.as_ref(), b"client");
    let mut settings = settings();
    settings.clients.get_mut(0).expect("client").id = boxed(b"client:+");
    settings.clients.get_mut(0).expect("client").secret = secret(b"s +:");
    let special = Issuer::new(settings, limits()).expect("special client");
    let body = replace(&body, "&client_id=client", "");
    let header = basic(b"client%3A%2B:s+%2B%3A");
    assert_eq!(
        special.token_request(Some(&header), TYPE, &body).expect("form encoded Basic").client.as_ref(),
        b"client:+"
    );
}
#[test]
fn redeem_binds_the_client_redirect_and_verifier() {
    let issuer = issuer();
    let request = issuer.token_request(None, TYPE, BODY).expect("request");
    let auth = issuer.authorize(QUERY).expect("auth");
    for field in 0_u8..3 {
        let mut other = auth.clone();
        match field {
            0 => other.client = boxed(b"other"),
            1 => other.redirect_uri = boxed(b"https://app.example.com/other"),
            2 => other.challenge[0] ^= 1,
            _ => unreachable!("field"),
        }
        assert_eq!(issuer.redeem(&request, &other), Err(TokenError::InvalidGrant), "field {field}");
    }
}
#[test]
fn token_refusal_answers_and_userinfo_refusal_are_byte_exact() {
    for (error, status, challenge, body) in [
        (TokenError::InvalidRequest, 400, None, b"{\"error\":\"invalid_request\"}".as_slice()),
        (TokenError::InvalidGrant, 400, None, b"{\"error\":\"invalid_grant\"}"),
        (TokenError::UnsupportedGrantType, 400, None, b"{\"error\":\"unsupported_grant_type\"}"),
        (TokenError::InvalidClient { header: false }, 401, None, b"{\"error\":\"invalid_client\"}"),
        (TokenError::InvalidClient { header: true }, 401, Some(b"Basic".as_slice()), b"{\"error\":\"invalid_client\"}"),
    ] {
        let answer = error.answer();
        assert_eq!((answer.status, answer.challenge.as_deref(), answer.body.as_ref()), (status, challenge, body));
    }
    let answer = issuer().userinfo_refused();
    assert_eq!(answer.status, 401);
    assert_eq!(answer.challenge.as_deref(), Some(b"Bearer error=\"invalid_token\"".as_slice()));
    assert_eq!(answer.body.as_ref(), b"");
}
#[test]
fn bearer_reads_only_the_bounded_rfc6750_spelling() {
    let issuer = issuer();
    for header in [
        None,
        Some(b"Basic x".as_slice()),
        Some(b"Bearer "),
        Some(b"Bearer  x"),
        Some(b"Bearer x y"),
        Some(b"Bearer x=y"),
        Some(b"Bearer ="),
        Some(b"Bearer x%"),
        Some(b"Bearer x\n"),
    ] {
        assert_eq!(issuer.bearer(header), None, "{header:?}");
    }
    assert_eq!(issuer.bearer(Some(b"bEaReR a-._~+/==")).expect("Bearer").expose(), b"a-._~+/==");
}
