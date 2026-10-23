use super::*;
use skein_lib::base64::{self, Alphabet};
#[test]
fn discovery_and_keys_are_byte_exact_and_the_kid_is_the_rfc7638_thumbprint() {
    let issuer = issuer();
    for (actual, fixture) in [(issuer.discovery(), "discovery.json"), (issuer.key_set(), "key-set.json")] {
        assert_eq!(
            actual.as_ref(),
            std::fs::read(std::path::Path::new("src/tests/fixtures").join(fixture)).expect("golden bytes")
        );
    }
    assert_eq!(issuer.kid.as_ref(), b"IsUn6_e04MaShXFIISMp4kG62LWzMIPy_MvSA5pJgX8");
    assert_eq!(issuer.exponent.as_ref(), b"AQAB");
    let mut settings = settings();
    settings.key = test_key(true);
    let second = Issuer::new(settings, limits()).expect("RFC 9500 key");
    assert_ne!(issuer.kid, second.kid, "replacement key names itself anew");
}
// Read the compact JWT member independently of its writer.
fn id_token(answer: &[u8]) -> Box<[u8]> {
    let marker = b"\"id_token\":\"";
    let start = bytes::find(answer, marker).expect("ID token member").checked_add(marker.len()).expect("offset");
    let rest = answer.get(start..).expect("member offset");
    let end = bytes::find(rest, b"\"").expect("closing quote");
    assert_eq!(rest.get(end..), Some(b"\"}".as_slice()), "ID token is the last member");
    boxed(rest.get(..end).expect("token bytes"))
}
#[test]
fn signed_id_tokens_carry_exact_claims_with_each_optional_value() {
    let issuer = issuer();
    let auth = issuer.authorize(QUERY).expect("auth");
    let public = skein_tls::rs256_public(test_key(false).expose()).expect("public");
    for mask in 0_u8..16 {
        let nonce = mask & 1 != 0;
        let mut auth = auth.clone();
        if nonce {
            auth.nonce = Some(boxed(b"nonce"));
        }
        let mut person = person();
        if mask & 2 == 0 {
            person.username = None;
        }
        if mask & 4 == 0 {
            person.email = None;
        }
        if mask & 8 == 0 {
            person.name = None;
        }
        let answer =
            issuer.tokens(&auth, &person, &secret(b"access"), Duration::from_secs(300), wall()).expect("tokens");
        assert!(
            answer.starts_with(
                b"{\"access_token\":\"access\",\"token_type\":\"Bearer\",\"expires_in\":300,\"id_token\":\""
            )
        );
        let jwt = id_token(&answer);
        let parts: Vec<_> = jwt.split(|byte| *byte == b'.').collect();
        assert_eq!(parts.len(), 3);
        let header = base64::decode(Alphabet::Url, parts[0], 1024).expect("header");
        assert_eq!(
            header.as_ref(),
            b"{\"alg\":\"RS256\",\"typ\":\"JWT\",\"kid\":\"IsUn6_e04MaShXFIISMp4kG62LWzMIPy_MvSA5pJgX8\"}"
        );
        let claims = base64::decode(Alphabet::Url, parts[1], 8192).expect("claims");
        let mut expected =
            b"{\"iss\":\"https://issuer.example\",\"sub\":\"person\",\"aud\":\"client\",\"exp\":400,\"iat\":100"
                .to_vec();
        if nonce {
            expected.extend_from_slice(b",\"nonce\":\"nonce\"");
        }
        if person.username.is_some() {
            expected.extend_from_slice(b",\"preferred_username\":\"username\"");
        }
        if person.email.is_some() {
            expected.extend_from_slice(b",\"email\":\"a@example.com\",\"email_verified\":true");
        }
        if person.name.is_some() {
            expected.extend_from_slice(b",\"name\":\"A Person\"");
        }
        expected.push(b'}');
        assert_eq!(claims.as_ref(), expected);
        let signature = base64::decode(Alphabet::Url, parts[2], 512).expect("signature");
        let input_len = jwt.len().checked_sub(parts[2].len()).expect("signature").checked_sub(1).expect("dot");
        assert!(skein_tls::verify_rs256(&public.modulus, &public.exponent, &jwt[..input_len], &signature), "signature");
    }
}
#[test]
fn userinfo_escapes_and_checks_each_person_string() {
    let issuer = issuer();
    assert_eq!(issuer.userinfo(&person()).expect("userinfo").as_ref(), b"{\"sub\":\"person\",\"preferred_username\":\"username\",\"email\":\"a@example.com\",\"email_verified\":true,\"name\":\"A Person\"}");
    let minimal = Person { subject: boxed(b"p\"\n"), username: None, email: None, name: None };
    assert_eq!(issuer.userinfo(&minimal).expect("escaping").as_ref(), b"{\"sub\":\"p\\\"\\n\"}");
    for field in 0_u8..4 {
        let mut person = person();
        match field {
            0 => person.subject = boxed(b"\xff"),
            1 => person.username = Some(boxed(b"\xff")),
            2 => person.email.as_mut().expect("email").address = boxed(b"\xff"),
            3 => person.name = Some(boxed(b"\xff")),
            _ => unreachable!("field"),
        }
        assert_eq!(issuer.userinfo(&person), Err(Error::Claims));
        assert_eq!(
            issuer.tokens(&authorization(), &person, &secret(b"access"), Duration::from_secs(1), wall()),
            Err(Error::Claims)
        );
    }
}

#[test]
fn empty_optional_strings_and_an_unverified_email_keep_their_claims() {
    let issuer = issuer();
    let person = Person {
        subject: boxed(b"p"),
        username: Some(boxed(b"")),
        email: Some(Email { address: boxed(b""), verified: false }),
        name: Some(boxed(b"")),
    };
    assert_eq!(
        issuer.userinfo(&person).expect("empty optional claims").as_ref(),
        b"{\"sub\":\"p\",\"preferred_username\":\"\",\"email\":\"\",\"email_verified\":false,\"name\":\"\"}"
    );
    let answer =
        issuer.tokens(&authorization(), &person, &secret(b"a"), Duration::from_secs(1), wall()).expect("tokens");
    let jwt = id_token(&answer);
    let payload = jwt.split(|byte| *byte == b'.').nth(1).expect("claims");
    let claims = base64::decode(Alphabet::Url, payload, 8192).expect("claims");
    assert_eq!(claims.as_ref(), b"{\"iss\":\"https://issuer.example\",\"sub\":\"p\",\"aud\":\"client\",\"exp\":101,\"iat\":100,\"preferred_username\":\"\",\"email\":\"\",\"email_verified\":false,\"name\":\"\"}");
}
