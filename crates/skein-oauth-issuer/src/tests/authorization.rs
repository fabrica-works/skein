use super::*;
#[test]
fn authorization_refusals_follow_the_contract_order() {
    let issuer = issuer();
    for (label, query, expected) in [
        ("bad escape", boxed(b"client_id=unknown&redirect_uri=other&x=%Q0"), Refusal::Malformed),
        ("truncated escape", boxed(b"client_id=unknown&x=%0"), Refusal::Malformed),
        ("client twice", boxed(b"client_id=client&client_id=other"), Refusal::Malformed),
        ("redirect twice", boxed(b"redirect_uri=a&redirect_uri=b"), Refusal::Malformed),
        ("client absent", boxed(b"redirect_uri=other"), Refusal::UnknownClient),
        ("client unknown", replace(QUERY, "client_id=client", "client_id=unknown"), Refusal::UnknownClient),
        ("redirect absent", boxed(b"client_id=client"), Refusal::UnknownRedirect),
        ("redirect unknown", replace(QUERY, "app.example.com", "other.example.com"), Refusal::UnknownRedirect),
        (
            "other duplicate",
            replace(QUERY, "scope=openid", "scope=openid&scope=openid"),
            back(b"invalid_request", None),
        ),
        ("ignored duplicate", replace(QUERY, "scope=openid", "scope=openid&x=1&x=2"), back(b"invalid_request", None)),
        ("type absent", replace(QUERY, "response_type=code", "response_type="), back(b"invalid_request", None)),
        ("method absent", replace(QUERY, "code_challenge_method=S256", "x=1"), back(b"invalid_request", None)),
        ("method plain", replace(QUERY, "method=S256", "method=plain"), back(b"invalid_request", None)),
        ("challenge absent", replace(QUERY, "code_challenge=E9", "x=E9"), back(b"invalid_request", None)),
        (
            "challenge short",
            replace(QUERY, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM", "x"),
            back(b"invalid_request", None),
        ),
        ("challenge alphabet", replace(QUERY, "E9Mel", "%2B9Mel"), back(b"invalid_request", None)),
        (
            "other type before scope",
            replace(QUERY, "response_type=code&scope=openid", "response_type=token&scope=other"),
            back(b"unsupported_response_type", None),
        ),
        ("scope missing", replace(QUERY, "scope=openid", "scope="), back(b"invalid_scope", None)),
        ("scope exact word", replace(QUERY, "scope=openid", "scope=xopenid"), back(b"invalid_scope", None)),
        (
            "request before URI",
            replace(QUERY, "scope=openid", "scope=openid&request=x&request_uri=x"),
            back(b"request_not_supported", None),
        ),
        (
            "request URI",
            replace(QUERY, "scope=openid", "scope=openid&request_uri=x"),
            back(b"request_uri_not_supported", None),
        ),
    ] {
        assert_eq!(issuer.authorize(&query), Err(expected), "{label}");
    }
}
#[test]
fn state_nonce_unknown_scopes_and_ignored_parameters_are_preserved_as_specified() {
    let issuer = issuer();
    let query = replace(
        QUERY,
        "scope=openid",
        "scope=profile++openid+email+unknown&state=a%2Bb%26c&nonce=n&prompt=none&max_age=1&login_hint=x&response_mode=query&request=",
    );
    let auth = issuer.authorize(&query).expect("ignored additions");
    assert_eq!(auth.state.as_deref(), Some(b"a+b&c".as_slice()));
    assert_eq!(auth.nonce.as_deref(), Some(b"n".as_slice()));
    assert_eq!(auth.challenge, *CHALLENGE, "RFC 7636 B");
    assert_eq!(
        issuer.redirect(&auth, &secret(b"a b+")).expect("redirect").as_ref(),
        b"https://app.example.com/callback?code=a%20b%2B&state=a%2Bb%26c"
    );
    let bad = replace(&query, "response_type=code", "response_type=token");
    assert_eq!(issuer.authorize(&bad), Err(back(b"unsupported_response_type", Some(b"a+b&c"))));
}
#[test]
fn a_registered_query_is_joined_with_an_ampersand() {
    let mut settings = settings();
    settings.clients.get_mut(0).expect("client").redirect_uri = boxed(b"https://app.example.com/callback?fixed=1");
    let issuer = Issuer::new(settings, limits()).expect("issuer");
    let query = replace(QUERY, "%2Fcallback", "%2Fcallback%3Ffixed%3D1");
    let auth = issuer.authorize(&query).expect("query redirect");
    assert_eq!(
        issuer.redirect(&auth, &secret(b"code")).expect("redirect").as_ref(),
        b"https://app.example.com/callback?fixed=1&code=code"
    );
    let refused = replace(&query, "response_type=code", "response_type=token");
    assert_eq!(
        issuer.authorize(&refused),
        Err(Refusal::Back {
            location: boxed(b"https://app.example.com/callback?fixed=1&error=unsupported_response_type")
        })
    );
}
#[test]
fn redirect_checks_a_caller_built_authorization_and_code() {
    let issuer = issuer();
    let auth = issuer.authorize(QUERY).expect("auth");
    for code in [b"".as_slice(), b"\x19", b"\x7f", b"\xff"] {
        assert_eq!(issuer.redirect(&auth, &secret(code)), Err(Error::Limits), "code {code:?}");
    }
    let mut other = auth.clone();
    other.client = boxed(b"other");
    assert_eq!(issuer.redirect(&other, &secret(b"code")), Err(Error::Client));
    let mut other = auth;
    other.redirect_uri = boxed(b"https://other.example/callback");
    assert_eq!(issuer.redirect(&other, &secret(b"code")), Err(Error::Client));
}

#[test]
fn every_refusal_wins_before_the_next_check_and_echoes_only_bounded_state() {
    let issuer = issuer();
    for (query, expected) in [
        (b"client_id=unknown&redirect_uri=x&client_id=client".as_slice(), Refusal::Malformed),
        (b"client_id=unknown&redirect_uri=x", Refusal::UnknownClient),
        (b"client_id=client&redirect_uri=x&scope=a&scope=b", Refusal::UnknownRedirect),
    ] {
        assert_eq!(issuer.authorize(query), Err(expected));
    }
    let bad_type = replace(QUERY, "response_type=code", "response_type=token");
    let invalid = replace(&bad_type, "method=S256", "method=plain");
    assert_eq!(issuer.authorize(&invalid), Err(back(b"invalid_request", None)));
    let bad_scope = replace(QUERY, "scope=openid", "scope=other&request=x&request_uri=x");
    assert_eq!(issuer.authorize(&bad_scope), Err(back(b"invalid_scope", None)));
    for length in [42, 43, 44] {
        let query = replace(QUERY, core::str::from_utf8(CHALLENGE).expect("ASCII"), &"a".repeat(length));
        assert_eq!(issuer.authorize(&query).is_ok(), length == 43, "challenge length {length}");
    }
    for length in [64, 65] {
        let state = "s".repeat(length);
        let query = [bad_type.as_ref(), format!("&state={state}").as_bytes()].concat();
        let refusal = if length == 64 {
            back(b"unsupported_response_type", Some(state.as_bytes()))
        } else {
            back(b"invalid_request", None)
        };
        assert_eq!(issuer.authorize(&query), Err(refusal));
    }
    let query = [QUERY, format!("&state=s&nonce={}", "n".repeat(65)).as_bytes()].concat();
    assert_eq!(issuer.authorize(&query), Err(back(b"invalid_request", Some(b"s"))));
    let query = [QUERY, b"&state=&nonce="].concat();
    let auth = issuer.authorize(&query).expect("empty values are absent");
    assert!(auth.state.is_none() && auth.nonce.is_none());
}
