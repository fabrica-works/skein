use super::*;
#[test]
fn settings_bounds_accept_the_limit_and_refuse_one_byte_past() {
    for field in 0_u8..8 {
        let mut limits = limits();
        let settings = settings();
        let len = match field {
            0 => settings.issuer.len(),
            1 => settings.authorization_endpoint.len(),
            2 => settings.token_endpoint.len(),
            3 => settings.userinfo_endpoint.len(),
            4 => settings.jwks_uri.len(),
            5 => settings.clients.get(0).expect("client").id.len(),
            6 => settings.clients.get(0).expect("client").secret.len(),
            7 => settings.clients.get(0).expect("client").redirect_uri.len(),
            _ => unreachable!("field"),
        };
        // Make every other value shorter, so this one alone reaches its cap.
        let mut settings = settings;
        settings.issuer = boxed(b"https://i");
        settings.authorization_endpoint = boxed(b"https://a");
        settings.token_endpoint = boxed(b"https://t");
        settings.userinfo_endpoint = boxed(b"https://u");
        settings.jwks_uri = boxed(b"https://k");
        let client = settings.clients.get_mut(0).expect("client");
        client.id = boxed(b"c");
        client.secret = secret(b"s");
        client.redirect_uri = boxed(b"https://r");
        let cap = u32::try_from(len).expect("length");
        if field <= 4 || field == 7 {
            limits.uri_bytes = cap;
        } else {
            limits.client_bytes = cap;
        }
        let value = vec![b'x'; len];
        match field {
            0 => settings.issuer = boxed(&[b"https://".as_slice(), &value[8..]].concat()),
            1 => settings.authorization_endpoint = boxed(&[b"https://".as_slice(), &value[8..]].concat()),
            2 => settings.token_endpoint = boxed(&[b"https://".as_slice(), &value[8..]].concat()),
            3 => settings.userinfo_endpoint = boxed(&[b"https://".as_slice(), &value[8..]].concat()),
            4 => settings.jwks_uri = boxed(&[b"https://".as_slice(), &value[8..]].concat()),
            5 => settings.clients.get_mut(0).expect("client").id = boxed(&value),
            6 => settings.clients.get_mut(0).expect("client").secret = secret(&value),
            7 => {
                settings.clients.get_mut(0).expect("client").redirect_uri =
                    boxed(&[b"https://".as_slice(), &value[8..]].concat());
            }
            _ => unreachable!("field"),
        }
        let accepted = Issuer::new(settings, limits).expect("exact bound");
        let mut past = accepted.settings;
        past.clients = List::with_capacity(2);
        for client in accepted.clients {
            past.clients.push(client).expect("slot");
        }
        match field {
            0 => past.issuer = [past.issuer.as_ref(), b"x"].concat().into_boxed_slice(),
            1 => past.authorization_endpoint = [past.authorization_endpoint.as_ref(), b"x"].concat().into_boxed_slice(),
            2 => past.token_endpoint = [past.token_endpoint.as_ref(), b"x"].concat().into_boxed_slice(),
            3 => past.userinfo_endpoint = [past.userinfo_endpoint.as_ref(), b"x"].concat().into_boxed_slice(),
            4 => past.jwks_uri = [past.jwks_uri.as_ref(), b"x"].concat().into_boxed_slice(),
            5 => {
                let c = past.clients.get_mut(0).expect("client");
                c.id = [c.id.as_ref(), b"x"].concat().into_boxed_slice();
            }
            6 => {
                let c = past.clients.get_mut(0).expect("client");
                c.secret = secret(&[c.secret.expose(), b"x"].concat());
            }
            7 => {
                let c = past.clients.get_mut(0).expect("client");
                c.redirect_uri = [c.redirect_uri.as_ref(), b"x"].concat().into_boxed_slice();
            }
            _ => unreachable!("field"),
        }
        assert_eq!(Issuer::new(past, limits).expect_err("past bound"), Error::Limits, "field {field}");
    }
    let mut limits = limits();
    limits.clients = 1;
    let issuer = Issuer::new(settings(), limits).expect("exact one client despite spare input capacity");
    assert_eq!(issuer.clients.len(), 1);
    let mut settings = settings();
    settings
        .clients
        .push(Client { id: boxed(b"other"), secret: secret(b"s"), redirect_uri: boxed(REDIRECT) })
        .expect("second");
    assert_eq!(Issuer::new(settings, limits).expect_err("past client count"), Error::Limits);
}
#[test]
fn request_header_state_nonce_and_token_bounds_are_checked_on_both_sides() {
    let issuer = issuer();
    for len in [2048_usize, 2049] {
        let mut query = QUERY.to_vec();
        query.extend_from_slice(b"&x=");
        query.resize(len, b'x');
        assert_eq!(issuer.authorize(&query).is_ok(), len == 2048, "query {len}");
        let mut body = BODY.to_vec();
        body.extend_from_slice(b"&x=");
        body.resize(len, b'x');
        assert_eq!(
            issuer.token_request(None, Some(b"application/x-www-form-urlencoded"), &body).is_ok(),
            len == 2048,
            "body {len}"
        );
        // A syntactically valid padded Basic value can include form escapes
        // in the id. The raw header cap is checked before parsing it.
        let mut header = vec![b'x'; len];
        header[..7].copy_from_slice(b"Bearer ");
        assert!(issuer.bearer(Some(&header)).is_none(), "token exceeds its own cap");
    }
    for name in ["state", "nonce"] {
        for len in [64_usize, 65] {
            let query = [QUERY, format!("&{name}={}", "s".repeat(len)).as_bytes()].concat();
            let result = issuer.authorize(&query);
            if len == 64 {
                assert!(result.is_ok(), "{name} at bound");
            } else {
                assert_eq!(result, Err(back(b"invalid_request", None)), "{name} past bound is not echoed");
            }
            let mut auth = issuer.authorize(QUERY).expect("auth");
            if name == "state" {
                auth.state = Some(boxed(&vec![b's'; len]));
            } else {
                auth.nonce = Some(boxed(&vec![b'n'; len]));
            }
            assert_eq!(issuer.redirect(&auth, &secret(b"c")).is_ok(), len == 64, "caller-built {name}");
        }
    }
    for len in [64_usize, 65] {
        let code = vec![b'c'; len];
        let auth = issuer.authorize(QUERY).expect("auth");
        assert_eq!(issuer.redirect(&auth, &secret(&code)).is_ok(), len == 64, "code bound");
        let body = replace(BODY, "code=code", &format!("code={}", "c".repeat(len)));
        let result = issuer.token_request(None, Some(b"application/x-www-form-urlencoded"), &body);
        if len == 64 {
            assert!(result.is_ok(), "code at bound");
        } else {
            assert_eq!(result.expect_err("code past"), TokenError::InvalidGrant);
        }
        let access = vec![b'a'; len];
        assert_eq!(
            issuer.tokens(&auth, &person(), &secret(&access), Duration::from_secs(1), wall()).is_ok(),
            len == 64,
            "access bound"
        );
        let header = [b"Bearer ".as_slice(), &access].concat();
        assert_eq!(issuer.bearer(Some(&header)).is_some(), len == 64, "bearer bound");
    }
    for len in [42_usize, 43, 128, 129] {
        let body = replace(BODY, core::str::from_utf8(VERIFIER).expect("ASCII"), &"v".repeat(len));
        assert_eq!(
            issuer.token_request(None, Some(b"application/x-www-form-urlencoded"), &body).is_ok(),
            (43..=128).contains(&len),
            "verifier {len}"
        );
    }
}
#[test]
fn claim_lengths_and_token_lifetime_are_bounded() {
    let issuer = issuer();
    let auth = issuer.authorize(QUERY).expect("auth");
    for field in 0_u8..4 {
        for len in [64_usize, 65] {
            let mut person = person();
            let value = boxed(&vec![b'x'; len]);
            match field {
                0 => person.subject = value,
                1 => person.username = Some(value),
                2 => person.email.as_mut().expect("email").address = value,
                3 => person.name = Some(value),
                _ => unreachable!("field"),
            }
            assert_eq!(issuer.userinfo(&person).is_ok(), len == 64, "claim {field} length {len}");
            assert_eq!(
                issuer.tokens(&auth, &person, &secret(b"a"), Duration::from_secs(1), wall()).is_ok(),
                len == 64,
                "token claim {field} length {len}"
            );
        }
    }
    for life in [Duration::ZERO, Duration::from_nanos(1), Duration::from_millis(1500)] {
        assert_eq!(issuer.tokens(&auth, &person(), &secret(b"a"), life, wall()), Err(Error::Limits));
    }
    for access in [b"".as_slice(), b"a+", b"a=", b"a "] {
        assert_eq!(
            issuer.tokens(&auth, &person(), &secret(access), Duration::from_secs(1), wall()),
            Err(Error::Limits)
        );
    }
    let mut person = person();
    person.subject = boxed(b"");
    assert_eq!(issuer.userinfo(&person), Err(Error::Claims));
}
#[test]
fn every_bound_at_once_fits_the_declared_heap() {
    use skein_heap::Span;
    let cap = limits();
    let bound = worst_case(&cap).expect("bound");
    let span = Span::start();
    let mut settings = settings();
    settings.clients.get_mut(0).expect("client").id = boxed(&[b'c'; 64]);
    settings.clients.get_mut(0).expect("client").secret = secret(&[b's'; 64]);
    settings
        .clients
        .push(Client {
            id: boxed(&[b'd'; 64]),
            secret: secret(&[b's'; 64]),
            redirect_uri: boxed(&[b"https://r/".as_slice(), &[b'x'; 246]].concat()),
        })
        .expect("client");
    for url in [
        &mut settings.issuer,
        &mut settings.authorization_endpoint,
        &mut settings.token_endpoint,
        &mut settings.userinfo_endpoint,
        &mut settings.jwks_uri,
        &mut settings.clients.get_mut(0).expect("client").redirect_uri,
    ] {
        *url = boxed(&[b"https://i/".as_slice(), &[b'x'; 246]].concat());
    }
    let issuer = Issuer::new(settings, cap).expect("issuer at bounds");
    let auth = Authorization {
        client: boxed(&[b'c'; 64]),
        redirect_uri: issuer.clients[0].redirect_uri.clone(),
        state: Some(boxed(&[1; 64])),
        nonce: Some(boxed(&[1; 64])),
        challenge: *CHALLENGE,
    };
    let person = Person {
        subject: boxed(&[1; 64]),
        username: Some(boxed(&[1; 64])),
        email: Some(Email { address: boxed(&[1; 64]), verified: true }),
        name: Some(boxed(&[1; 64])),
    };
    let answer =
        issuer.tokens(&auth, &person, &secret(&[b'a'; 64]), Duration::from_secs(300), wall()).expect("largest answer");
    let discovery = issuer.discovery();
    let key_set = issuer.key_set();
    let redirect = issuer.redirect(&auth, &secret(&[b'c'; 64])).expect("redirect");
    let info = issuer.userinfo(&person).expect("userinfo");
    let measured = span.end();
    assert!(u64::try_from(measured.peak).expect("positive heap") <= bound, "issuer and largest escaped token call");
    drop((answer, discovery, key_set, redirect, info));
}

#[test]
fn valid_authentication_headers_reach_the_request_limit_and_stop_past_it() {
    let encoded_secret = "%73".repeat(64);
    let header = token::basic(format!("client:{encoded_secret}").as_bytes());
    let body = replace(BODY, "&client_secret=secret", "");
    for cap in [header.len(), header.len() - 1] {
        let mut limits = limits();
        limits.request_bytes = u32::try_from(cap).expect("header length");
        let mut config = settings();
        config.clients.get_mut(0).expect("client").secret = secret(&[b's'; 64]);
        let issuer = Issuer::new(config, limits).expect("issuer");
        let request = issuer.token_request(Some(&header), Some(b"application/x-www-form-urlencoded"), &body);
        if cap == header.len() {
            assert_eq!(request.expect("header at bound").client.as_ref(), b"client");
        } else {
            assert_eq!(request.expect_err("header past bound"), TokenError::InvalidRequest);
        }
    }
    let header = [b"Bearer ".as_slice(), &[b'a'; 64]].concat();
    for cap in [header.len(), header.len() - 1] {
        let mut limits = limits();
        limits.request_bytes = u32::try_from(cap).expect("header length");
        let issuer = Issuer::new(settings(), limits).expect("issuer");
        assert_eq!(issuer.bearer(Some(&header)).is_some(), cap == header.len());
    }
}

#[test]
fn form_fields_at_the_request_bound_fit_the_declared_heap() {
    use skein_heap::Span;
    let limits = limits();
    let start = Span::start();
    let issuer = issuer();
    let held = start.end().net;
    let body = vec![b'&'; usize::try_from(limits.request_bytes).expect("bound")];
    let span = Span::start();
    let result = issuer.token_request(None, Some(b"application/x-www-form-urlencoded"), &body);
    let measured = span.end();
    assert_eq!(result.expect_err("no grant"), TokenError::InvalidRequest);
    assert!(
        u64::try_from(held.checked_add(measured.peak).expect("heap sum")).expect("positive heap")
            <= worst_case(&limits).expect("bound")
    );
}
