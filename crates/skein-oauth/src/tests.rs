use crate::*;
use alloc::boxed::Box;
use skein_json::Token;
use skein_lib::{Duration, Wall, bytes};

// Synthetic token metadata, not provider captures. Signatures are placeholders;
// production read_claims deliberately supplies no signature authentication.
const WITH_EXP: &[u8] =
    b"e30.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdC03In0sImV4cCI6MTIwfQ.AA";
const NO_EXP: &[u8] = b"e30.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdC03In19.AA";
const BAD_ACCOUNT: &[u8] =
    b"e30.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYmFkXHJcbmhlYWRlciJ9fQ.AA";
const DUPLICATE: &[u8] = b"e30.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYSIsImNoYXRncHRfYWNjb3VudF9pZCI6ImIifX0.AA";
fn limits() -> Limits {
    Limits {
        document_bytes: 2048,
        string_bytes: 512,
        token_bytes: 512,
        client_bytes: 64,
        detail_bytes: 64,
        record_bytes: 4096,
        depth: 8,
        tokens: 128,
        issued_client_bytes: 64,
        granted_bytes: 64,
        subject_bytes: 64,
        id_token_bytes: 1024,
    }
}
fn boxed(input: &[u8]) -> Box<[u8]> {
    bytes::copy_of(input)
}
fn parse(input: &[u8]) -> Json {
    Json::from_bytes(input, &limits()).expect("synthetic valid JSON")
}
fn wall(seconds: u64) -> Wall {
    Wall::from_nanos(seconds.checked_mul(1_000_000_000).expect("test seconds fit"))
}
fn selector() -> ClaimSelector {
    ClaimSelector { object: boxed(b"https://api.openai.com/auth"), field: boxed(b"chatgpt_account_id") }
}
fn request() -> RefreshRequest {
    RefreshRequest { client_id: boxed(b"client-1"), refresh_token: boxed(b"refresh-old") }
}
fn response() -> TokenResponse {
    TokenResponse {
        access_token: boxed(b"access-new"),
        refresh_token: Some(boxed(b"refresh-new")),
        expires_in: 30,
        scope: None,
        id_token: None,
        earliest_refresh_at: None,
    }
}
fn record() -> SavedToken {
    SavedToken {
        key: 7,
        generation: 9,
        access_token: boxed(b"access-old"),
        refresh_token: Some(boxed(b"refresh-old")),
        metadata: None,
        expires_at: wall(100),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
        earliest_refresh_at: None,
    }
}

#[test]
fn refresh_request_is_measured_and_matches_the_wire_contract() {
    let request = request();
    let expected = br#"{"grant_type":"refresh_token","client_id":"client-1","refresh_token":"refresh-old"}"#;
    assert_eq!(encode_request(&request, &limits()).expect("encode"), boxed(expected));
    assert!(decode_request(&parse(expected), &limits()).expect("decode") == request);
    let escaped = RefreshRequest { client_id: boxed(b"c\"\n"), refresh_token: boxed(b"r\\\t") };
    let body = encode_request(&escaped, &limits()).expect("escaping");
    assert!(decode_request(&parse(&body), &limits()).expect("unescape") == escaped);
    let mut exact = limits();
    exact.document_bytes = u32::try_from(expected.len()).expect("fits");
    assert_eq!(encode_request(&request, &exact).expect("exact cap"), boxed(expected));
    exact.document_bytes = exact.document_bytes.checked_sub(1).expect("nonzero");
    assert_eq!(encode_request(&request, &exact), Err(DecodeError::TooLarge));
}

#[test]
fn server_response_supports_both_rotating_and_stable_refresh_tokens() {
    let response = response();
    let body = encode_response(&response, &limits()).expect("encode");
    assert_eq!(
        body.as_ref(),
        br#"{"access_token":"access-new","token_type":"Bearer","refresh_token":"refresh-new","expires_in":30}"#
    );
    assert!(decode_response(&parse(&body), &limits()).expect("decode") == response);
    let stable = parse(br#"{"access_token":"access-new","expires_in":30,"ignored":[1,true,null]}"#);
    let expected = TokenResponse { refresh_token: None, ..response };
    assert!(decode_response(&stable, &limits()).expect("optional token_type and refresh_token") == expected);
    assert!(
        decode_response(&parse(br#"{"access_token":"a","token_type":"MAC","expires_in":30}"#), &limits())
            == Err(DecodeError::WrongType)
    );
    assert!(
        decode_response(&parse(br#"{"access_token":"a","expires_in":0}"#), &limits()) == Err(DecodeError::Malformed)
    );
    assert!(
        decode_response(&parse(br#"{"access_token":"a","expires_in":-1}"#), &limits()) == Err(DecodeError::WrongType)
    );
    assert!(
        decode_response(&parse(br#"{"access_token":"a","refresh_token":null,"expires_in":1}"#), &limits())
            == Err(DecodeError::WrongType)
    );
    assert!(
        decode_response(&parse(br#"{"access_token":"a","expires_in":1,"expires_in":2}"#), &limits())
            == Err(DecodeError::Malformed)
    );
}

#[test]
fn bounded_decoders_refuse_wrong_grants_tokens_and_larger_preparsed_documents() {
    let mut bounded = limits();
    bounded.client_bytes = 1;
    assert!(
        decode_request(&parse(&encode_request(&request(), &limits()).expect("encode")), &bounded)
            == Err(DecodeError::TooLarge)
    );
    bounded = limits();
    bounded.token_bytes = 1;
    assert!(
        decode_response(&parse(&encode_response(&response(), &limits()).expect("encode")), &bounded)
            == Err(DecodeError::TooLarge)
    );
    let value = parse(&encode_request(&request(), &limits()).expect("encode"));
    bounded = limits();
    bounded.document_bytes = 10;
    assert!(decode_request(&value, &bounded) == Err(DecodeError::TooLarge));
    bounded = limits();
    bounded.tokens = 3;
    assert!(decode_request(&value, &bounded) == Err(DecodeError::TooLarge));
    bounded = limits();
    bounded.depth = 0;
    assert!(decode_request(&value, &bounded) == Err(DecodeError::TooLarge));
    bounded = limits();
    bounded.string_bytes = 4;
    assert!(decode_request(&value, &bounded) == Err(DecodeError::TooLarge));
    assert!(
        decode_request(&parse(br#"{"grant_type":"password","client_id":"c","refresh_token":"r"}"#), &limits())
            == Err(DecodeError::Malformed)
    );
    assert_eq!(
        encode_response(&TokenResponse { access_token: boxed(b"x\r\nheader"), ..response() }, &limits()),
        Err(DecodeError::Malformed)
    );
    assert_eq!(
        encode_response(&TokenResponse { access_token: boxed(b"==="), ..response() }, &limits()),
        Err(DecodeError::Malformed)
    );
    assert_eq!(
        encode_request(&RefreshRequest { refresh_token: boxed(&[0xff]), ..request() }, &limits()),
        Err(DecodeError::Malformed)
    );
}

#[test]
fn error_code_and_transport_outcome_classification_keep_refusal_distinct() {
    let error = OAuthError { code: boxed(b"invalid_grant"), detail: boxed(b"refresh token already spent") };
    let encoded = encode_error(&error, &limits()).expect("encode");
    assert_eq!(decode_error(&parse(&encoded), &limits()).expect("decode"), error);
    assert_eq!(classify(400, Some(&error), Duration::ZERO), Failure::Refused);
    assert_eq!(classify(401, None, Duration::ZERO), Failure::ClientRejected);
    assert_eq!(
        classify(429, None, Duration::from_secs(12)),
        Failure::RateLimited { retry_after: Duration::from_secs(12) }
    );
    assert_eq!(classify(503, None, Duration::ZERO), Failure::TimedOut);
    let temporary = OAuthError { code: boxed(b"temporarily_unavailable"), detail: boxed(b"") };
    assert_eq!(classify(400, Some(&temporary), Duration::ZERO), Failure::TimedOut);
    let mut bounded = limits();
    bounded.detail_bytes = 3;
    let short = decode_error(&parse(br#"{"error":"x","error_description":"ab\u00e9cd"}"#), &bounded)
        .expect("truncate UTF8 safely");
    assert_eq!(short.detail.as_ref(), b"ab");
}

#[test]
fn jwt_claims_extract_selected_metadata_once_and_use_injected_wall_time() {
    let claims = read_claims(WITH_EXP, &selector(), wall(100), &limits()).expect("claims");
    assert_eq!(claims.metadata.as_ref(), b"acct-7");
    assert_eq!(claims.expires_at, Some(wall(120)));
    assert_eq!(claims.valid, Some(Duration::from_secs(20)));
    assert!(
        read_claims(WITH_EXP, &selector(), wall(130), &limits()).expect("expired metadata").valid
            == Some(Duration::ZERO)
    );
    let optional = read_claims(NO_EXP, &selector(), wall(100), &limits()).expect("no expiry claim");
    assert_eq!(optional.expires_at, None);
    assert_eq!(optional.valid, None);
    assert!(read_claims(BAD_ACCOUNT, &selector(), wall(100), &limits()) == Err(DecodeError::Malformed));
    assert!(read_claims(DUPLICATE, &selector(), wall(100), &limits()) == Err(DecodeError::Malformed));
    let mut bounded = limits();
    bounded.document_bytes = 40;
    assert!(read_claims(WITH_EXP, &selector(), wall(100), &bounded) == Err(DecodeError::TooLarge));
    bounded = limits();
    bounded.token_bytes = 30;
    assert!(read_claims(WITH_EXP, &selector(), wall(100), &bounded) == Err(DecodeError::TooLarge));
}

#[test]
fn base64url_mutations_and_claim_shape_errors_are_refused_without_authentication() {
    // Invalid remainder, noncanonical tail bits, padding, extra separators,
    // forbidden alphabet and valid JSON with missing/wrong-shaped claims.
    for input in [
        b"e30.A.AA".as_slice(),
        b"e30.AB.AA",
        b"e30.AAB.AA",
        b"e30.e30=.AA",
        b"e30.e30.AA.extra",
        b"e30.e+0.AA",
        b"e30.e30.AA",
        b"e30.W10.AA",
        b".e30.AA",
        b"e30..AA",
        b"e30.e30.",
    ] {
        let outcome = read_claims(input, &selector(), wall(100), &limits());
        let refused = outcome.is_err();
        assert!(refused, "malformed metadata must be refused");
    }
}

#[test]
fn an_access_only_record_round_trips_without_refresh_state() {
    let record = SavedToken {
        key: 1,
        generation: 2,
        access_token: boxed(b"a"),
        refresh_token: None,
        metadata: None,
        expires_at: Wall::from_nanos(3),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
        earliest_refresh_at: None,
    };
    let golden = b"SKOT\x00\x03\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x01a\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    assert_eq!(encode_record(&record, &limits()).expect("access-only encoding").as_ref(), golden);
    let restored = decode_record(golden, &limits()).expect("access-only record");
    assert!(restored == record);
    assert!(record.refresh_state().is_none());
    assert!(restored.refresh_state().is_none());
    assert_eq!(restored.remaining(Wall::from_nanos(1)), Duration::from_nanos(2));
    let mut old = boxed(golden);
    *old.get_mut(5).expect("version byte") = 1;
    assert!(decode_record(&old, &limits()) == Err(DecodeError::Version));
}

#[test]
fn record_bounds_hold_with_and_without_a_refresh_token() {
    for refresh_token in [None, Some(boxed(&[b'r'; 512]))] {
        let mut record = SavedToken { access_token: boxed(&[b'a'; 512]), refresh_token, ..record() };
        let encoded = encode_record(&record, &limits()).expect("token caps");
        let mut exact = limits();
        exact.record_bytes = u32::try_from(encoded.len()).expect("bounded length");
        assert!(decode_record(&encoded, &exact).expect("exact record cap") == record);
        assert_eq!(encode_record(&record, &exact).expect("exact record cap"), encoded);
        exact.record_bytes = exact.record_bytes.checked_sub(1).expect("record is nonempty");
        assert_eq!(encode_record(&record, &exact), Err(DecodeError::TooLarge));
        assert!(decode_record(&encoded, &exact) == Err(DecodeError::TooLarge));
        record.access_token = boxed(&[b'a'; 513]);
        assert_eq!(encode_record(&record, &limits()), Err(DecodeError::TooLarge));
    }
    let mut record = record();
    assert!(record.refresh_state().is_some());
    record.refresh_token = Some(boxed(b""));
    assert_eq!(encode_record(&record, &limits()), Err(DecodeError::Malformed));
    record.refresh_token = Some(boxed(&[b'r'; 513]));
    assert_eq!(encode_record(&record, &limits()), Err(DecodeError::TooLarge));
}

#[test]
fn saved_record_has_an_independent_big_endian_versioned_golden_and_refuses_corruption() {
    let record = SavedToken {
        key: 1,
        generation: 2,
        access_token: boxed(b"a"),
        refresh_token: Some(boxed(b"r")),
        metadata: None,
        expires_at: Wall::from_nanos(3),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
        earliest_refresh_at: None,
    };
    let golden = b"SKOT\x00\x03\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x01a\x00\x00\x00\x01r\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    assert_eq!(encode_record(&record, &limits()).expect("encode").as_ref(), golden);
    assert!(decode_record(golden, &limits()).expect("golden") == record);
    for len in 0..golden.len() {
        let outcome = decode_record(golden.get(..len).expect("prefix"), &limits());
        let refused = outcome.is_err();
        assert!(refused, "every truncated record is refused");
    }
    let mut version = boxed(golden);
    *version.get_mut(5).expect("version low byte") = 1;
    assert!(decode_record(&version, &limits()) == Err(DecodeError::Version));
    let mut length = boxed(golden);
    *length.get_mut(26).expect("access length high byte") = 0xff;
    assert!(decode_record(&length, &limits()) == Err(DecodeError::TooLarge));
    let mut bounded = limits();
    bounded.record_bytes = 39;
    assert_eq!(encode_record(&record, &bounded), Err(DecodeError::TooLarge));
    assert!(decode_record(golden, &bounded) == Err(DecodeError::TooLarge));
}

#[test]
fn refresh_only_startup_retains_or_rotates_refresh_and_resumes_saved_generation() {
    let starting = RefreshState {
        key: 7,
        generation: 0,
        refresh_token: boxed(b"refresh-old"),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
    };
    let mut answer = response();
    answer.refresh_token = None;
    let first = rotate(&starting, &answer, None, wall(100), &limits()).expect("first refresh requires no access token");
    assert_eq!(first.generation, 1);
    assert_eq!(first.refresh_token.as_deref().expect("refresh token"), b"refresh-old");
    assert_eq!(first.remaining(wall(100)), Duration::from_secs(30));
    assert_eq!(first.remaining(wall(112)), Duration::from_secs(18));
    assert_eq!(first.remaining(wall(140)), Duration::ZERO);
    let saved = encode_record(&first, &limits()).expect("candidate kept");
    let restarted = decode_record(&saved, &limits()).expect("restart");
    let next =
        rotate(&restarted.refresh_state().expect("record has refresh state"), &response(), None, wall(115), &limits())
            .expect("after restart");
    assert_eq!(next.generation, 2);
    assert_eq!(next.refresh_token.as_deref().expect("refresh token"), b"refresh-new");
    assert_eq!(next.expires_at, wall(145));
    assert!(restarted == first, "a candidate does not mutate the kept generation");
}

#[test]
fn jwt_rotation_keeps_claims_and_expiry_but_checked_overflow_never_reuses_a_name() {
    let previous = record();
    let answer = TokenResponse { access_token: boxed(WITH_EXP), ..response() };
    let candidate = rotate(
        &previous.refresh_state().expect("record has refresh state"),
        &answer,
        Some(&selector()),
        wall(100),
        &limits(),
    )
    .expect("claims accompany access");
    assert_eq!(candidate.metadata, Some(boxed(b"acct-7")));
    assert_eq!(candidate.expires_at, wall(120), "JWT expiry and exchange validity both cap grants");
    assert!(
        decode_record(&encode_record(&candidate, &limits()).expect("kept"), &limits()).expect("restored") == candidate
    );
    let mut full = previous.refresh_state().expect("record has refresh state");
    full.generation = u64::MAX;
    assert!(rotate(&full, &answer, Some(&selector()), wall(100), &limits()) == Err(DecodeError::TooLarge));
    assert!(
        rotate(
            &previous.refresh_state().expect("record has refresh state"),
            &response(),
            None,
            Wall::from_nanos(u64::MAX),
            &limits()
        ) == Err(DecodeError::TooLarge)
    );
    assert!(
        rotate(
            &previous.refresh_state().expect("record has refresh state"),
            &TokenResponse { expires_in: u64::MAX, ..response() },
            None,
            wall(100),
            &limits()
        ) == Err(DecodeError::TooLarge)
    );
}

#[test]
fn token_collector_rejects_malformed_grammar_and_counts_all_document_caps() {
    assert_eq!(
        Json::from_tokens(&[Token::ObjectStart, Token::Key(boxed(b"x")), Token::ObjectEnd], &limits()),
        Err(DecodeError::Malformed)
    );
    assert_eq!(Json::from_tokens(&[Token::String(boxed(&[0xff]))], &limits()), Err(DecodeError::Malformed));
    let mut collector = Collector::new(&limits());
    collector.push(Token::ObjectStart).expect("start");
    collector.push(Token::Key(boxed(b"x"))).expect("key");
    assert_eq!(collector.finish(&limits()), Err(DecodeError::Malformed));
    let bound = worst_case(&limits()).expect("checked bound");
    assert!(bound > 15_000, "formula includes tokens, candidates, two generations, documents and saved buffers");
}

fn additions(present: [bool; 5]) -> SavedToken {
    let mut value = record();
    value.issued_client_id = present[0].then_some(boxed(b"client"));
    value.granted_scopes = present[1].then_some(boxed(b"read"));
    value.subject = present[2].then_some(boxed(b"subject"));
    value.id_token = present[3].then_some(boxed(b"header.payload.signature"));
    value.earliest_refresh_at = present[4].then_some(wall(90));
    value
}
#[test]
fn a_record_round_trips_each_addition() {
    for (name, present) in [
        ("issued", [true, false, false, false, false]),
        ("granted", [false, true, false, false, false]),
        ("subject", [false, false, true, false, false]),
        ("id", [false, false, false, true, false]),
        ("earliest", [false, false, false, false, true]),
        ("all", [true; 5]),
        ("none", [false; 5]),
        ("granted empty", [false; 5]),
    ] {
        let mut value = additions(present);
        if name == "granted empty" {
            value.granted_scopes = Some(boxed(b""));
        }
        let encoded = encode_record(&value, &limits()).expect("encoded");
        assert!(decode_record(&encoded, &limits()).expect("decoded") == value, "{name}");
    }
}
const VERSION_TWO: &[u8] = b"SKOT\x00\x02\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x01a\x00\x00\x00\x01r\x00\x00\x00\x00";
fn byte_record() -> SavedToken {
    SavedToken {
        key: 1,
        generation: 2,
        access_token: boxed(b"a"),
        refresh_token: Some(boxed(b"r")),
        metadata: None,
        expires_at: Wall::from_nanos(3),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
        earliest_refresh_at: None,
    }
}
#[test]
fn a_version_two_record_reads_with_no_additions() {
    assert!(decode_record(VERSION_TWO, &limits()).expect("version two") == byte_record());
}
#[test]
fn a_version_three_record_reads_its_bytes() {
    let bytes = b"SKOT\x00\x03\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x01a\x00\x00\x00\x01r\x00\x00\x00\x00\x01\x00\x00\x00\x01c\x01\x00\x00\x00\x00\x01\x00\x00\x00\x01s\x01\x00\x00\x00\x01i\x01\x00\x00\x00\x00\x00\x00\x00\x04";
    let expected = SavedToken {
        issued_client_id: Some(boxed(b"c")),
        granted_scopes: Some(boxed(b"")),
        subject: Some(boxed(b"s")),
        id_token: Some(boxed(b"i")),
        earliest_refresh_at: Some(Wall::from_nanos(4)),
        ..byte_record()
    };
    assert!(decode_record(bytes, &limits()).expect("version three") == expected);
    assert!(encode_record(&expected, &limits()).expect("encoded").as_ref() == bytes);
}
#[test]
fn a_record_of_another_version_is_refused() {
    for version in [1, 4] {
        let mut input = boxed(VERSION_TWO);
        input[5] = version;
        assert!(decode_record(&input, &limits()) == Err(DecodeError::Version), "version {version}");
    }
}
#[test]
fn a_token_response_reads_scope_id_token_and_earliest() {
    let input = br#"{"access_token":"access-new","refresh_token":"refresh-new","expires_in":30,"scope":"read","id_token":"header.payload.signature","earliest_refresh_at":90}"#;
    let expected = TokenResponse {
        scope: Some(boxed(b"read")),
        id_token: Some(boxed(b"header.payload.signature")),
        earliest_refresh_at: Some(wall(90)),
        ..response()
    };
    assert!(decode_response(&parse(input), &limits()).expect("response") == expected);
    let encoded = encode_response(&expected, &limits()).expect("encoded");
    assert!(decode_response(&parse(&encoded), &limits()).expect("round trip") == expected);
}
#[test]
fn an_unreadable_earliest_refresh_time_is_ignored() {
    for (name, input) in [
        ("string", br#"{"access_token":"a","expires_in":1,"earliest_refresh_at":"90"}"#.as_slice()),
        ("negative", br#"{"access_token":"a","expires_in":1,"earliest_refresh_at":-1}"#),
        ("fraction", br#"{"access_token":"a","expires_in":1,"earliest_refresh_at":1.5}"#),
        ("overflow", br#"{"access_token":"a","expires_in":1,"earliest_refresh_at":18446744074}"#),
    ] {
        assert!(decode_response(&parse(input), &limits()).expect("response").earliest_refresh_at.is_none(), "{name}");
    }
}
#[test]
fn a_twice_named_addition_is_malformed() {
    for (name, input) in [
        ("scope", br#"{"access_token":"a","expires_in":1,"scope":"","scope":""}"#.as_slice()),
        ("id_token", br#"{"access_token":"a","expires_in":1,"id_token":"i","id_token":"i"}"#),
        ("earliest", br#"{"access_token":"a","expires_in":1,"earliest_refresh_at":1,"earliest_refresh_at":2}"#),
    ] {
        assert!(decode_response(&parse(input), &limits()) == Err(DecodeError::Malformed), "{name}");
    }
}
#[test]
fn each_new_cap_holds_at_its_boundary() {
    for (name, field, cap) in [("issued", 0_u8, 64), ("granted", 1, 64), ("subject", 2, 64), ("id", 3, 1024)] {
        for extra in [0, 1] {
            let mut value = record();
            let bytes = Some(boxed(&[b'x'; 1025][..cap + extra]));
            match field {
                0 => value.issued_client_id = bytes,
                1 => value.granted_scopes = bytes,
                2 => value.subject = bytes,
                3 => value.id_token = bytes,
                _ => unreachable!(),
            }
            if extra == 0 {
                let encoded = encode_record(&value, &limits()).expect("at cap");
                assert!(decode_record(&encoded, &limits()).expect("at cap") == value, "{name}");
            } else {
                assert!(encode_record(&value, &limits()) == Err(DecodeError::TooLarge), "{name}");
                let prior = value.refresh_state().expect("refreshable");
                assert!(
                    rotate(&prior, &response(), None, wall(100), &limits()) == Err(DecodeError::TooLarge),
                    "{name}"
                );
                let expanded = Limits {
                    issued_client_bytes: 65,
                    granted_bytes: 65,
                    subject_bytes: 65,
                    id_token_bytes: 1025,
                    ..limits()
                };
                let encoded = encode_record(&value, &expanded).expect("larger cap");
                assert!(decode_record(&encoded, &limits()) == Err(DecodeError::TooLarge), "{name}");
            }
        }
    }
}

fn id_expected() -> jwt::Expected<'static> {
    jwt::Expected { issuer: b"https://issuer.example", client_id: b"client", nonce: b"n-0123456789abcdef" }
}
fn claims(exp: u64) -> Box<[u8]> {
    claims_with(b"https://issuer.example", br#""client""#, exp, b"n-0123456789abcdef", Some(b"user-42"), None)
}
fn claims_with(
    issuer: &[u8],
    audience: &[u8],
    exp: u64,
    nonce: &[u8],
    subject: Option<&[u8]>,
    azp: Option<&[u8]>,
) -> Box<[u8]> {
    use skein_lib::{Decimal, Writer};
    let digits = Decimal::of(exp);
    let parts: [&[u8]; 14] = [
        br#"{"iss":""#,
        issuer,
        br#"","aud":"#,
        audience,
        br#", "exp":"#,
        digits.as_bytes(),
        br#", "nonce":""#,
        nonce,
        b"\"",
        if subject.is_some() { br#", "sub":""# } else { b"" },
        subject.unwrap_or(b""),
        if subject.is_some() { b"\"" } else { b"" },
        if azp.is_some() { br#", "azp":""# } else { b"" },
        azp.unwrap_or(b""),
    ];
    let tail: &[u8] = if azp.is_some() { b"\"}" } else { b"}" };
    let mut size = tail.len();
    for part in parts {
        size = size.checked_add(part.len()).expect("small fixture");
    }
    let mut out = Writer::new(size);
    for part in parts {
        out.put(part).expect("measured");
    }
    out.put(tail).expect("measured");
    out.finish()
}
fn jws(header: &[u8], payload: &[u8], signature: &[u8]) -> Box<[u8]> {
    use skein_lib::{
        Writer,
        base64::{self, Alphabet},
    };
    let mut length = 2_u32;
    for part in [header, payload, signature] {
        let encoded = base64::encoded_len(Alphabet::Url, u32::try_from(part.len()).expect("fixture")).expect("fixture");
        length = length.checked_add(encoded).expect("small fixture");
    }
    let mut out = Writer::new(usize::try_from(length).expect("fixture"));
    for (index, part) in [header, payload, signature].into_iter().enumerate() {
        if index != 0 {
            out.put(b".").expect("measured");
        }
        base64::encode(Alphabet::Url, part, &mut out).expect("measured");
    }
    out.finish()
}
#[test]
fn an_id_tokens_alg_must_be_rs256() {
    for (name, header, expected) in [
        ("none", br#"{"alg":"none"}"#.as_slice(), Check::Signature),
        ("HS256", br#"{"alg":"HS256"}"#, Check::Signature),
        ("RS384", br#"{"alg":"RS384"}"#, Check::Signature),
        ("RS256", br#"{"alg":"RS256"}"#, Check::Keys),
    ] {
        let token = jws(header, &claims(1100), b"signature");
        let got = jwt::verify_id_token(&token, &[], &id_expected(), wall(1000), Duration::ZERO, &limits());
        assert_eq!(got.err(), Some(expected), "{name}");
    }
}
#[test]
fn the_key_is_chosen_by_kid_else_the_one_key() {
    fn key(kid: &[u8]) -> keys::Key {
        keys::Key { kid: Some(boxed(kid)), modulus: boxed(b"n"), exponent: boxed(b"e") }
    }
    let keys = [key(b"first"), key(b"second")];
    for (name, kid, count, expected) in [
        ("named", Some(b"second".as_slice()), 2, Some(1)),
        ("unknown", Some(b"unknown".as_slice()), 2, None),
        ("one", None, 1, Some(0)),
        ("two", None, 2, None),
    ] {
        let held = keys.get(..count).expect("fixture");
        let got = match jwt::pick_key(kid, held) {
            Ok(key) => {
                let mut found = None;
                for (index, candidate) in held.iter().enumerate() {
                    if core::ptr::eq(key, candidate) {
                        found = Some(index);
                    }
                }
                found
            }
            Err(_) => None,
        };
        assert_eq!(got, expected, "{name}");
    }
}
#[test]
fn each_claim_is_checked() {
    for (name, payload, expected) in [
        (
            "issuer",
            claims_with(b"another", br#""client""#, 1100, b"n-0123456789abcdef", Some(b"user-42"), None),
            Check::Issuer,
        ),
        (
            "audience",
            claims_with(
                b"https://issuer.example",
                br#""another""#,
                1100,
                b"n-0123456789abcdef",
                Some(b"user-42"),
                None,
            ),
            Check::Audience,
        ),
        ("expiry", claims(1000), Check::Expiry),
        (
            "nonce",
            claims_with(b"https://issuer.example", br#""client""#, 1100, b"another", Some(b"user-42"), None),
            Check::Nonce,
        ),
        (
            "subject",
            claims_with(b"https://issuer.example", br#""client""#, 1100, b"n-0123456789abcdef", None, None),
            Check::Missing,
        ),
    ] {
        let got = jwt::check_claims(&payload, &id_expected(), wall(1000), Duration::ZERO, &limits());
        assert_eq!(got.err(), Some(expected), "{name}");
    }
}
#[test]
fn aud_may_be_a_string_or_an_array() {
    for (name, audience, azp, expected) in [
        ("string", br#""client""#.as_slice(), None, None),
        ("one in array", br#"["client"]"#, None, None),
        ("two with client azp", br#"["client","other"]"#, Some(b"client".as_slice()), None),
        ("two with another azp", br#"["client","other"]"#, Some(b"other".as_slice()), Some(Check::Audience)),
        ("two without azp", br#"["client","other"]"#, None, Some(Check::Audience)),
    ] {
        let payload =
            claims_with(b"https://issuer.example", audience, 1100, b"n-0123456789abcdef", Some(b"user-42"), azp);
        let got = jwt::check_claims(&payload, &id_expected(), wall(1000), Duration::ZERO, &limits());
        assert_eq!(got.err(), expected, "{name}");
    }
}
#[test]
fn an_expiry_on_the_skews_edge_is_expired_and_a_second_later_is_not() {
    for (skew, exp, expected) in
        [(60, 940, Some(Check::Expiry)), (60, 941, None), (0, 1000, Some(Check::Expiry)), (0, 1001, None)]
    {
        let got = jwt::check_claims(&claims(exp), &id_expected(), wall(1000), Duration::from_secs(skew), &limits());
        assert_eq!(got.err(), expected, "skew {skew}, exp {exp}");
    }
}

const N: &str = "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ";
const INPUT: &[u8] = b"eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ";
const SIGNATURE: &str = "cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqvhJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrBp0igcN_IoypGlUPQGe77Rw";
#[test]
fn rfc_7515_a2_passes_the_signature_check() {
    use skein_lib::base64::{self, Alphabet};
    let key = keys::Key {
        kid: None,
        modulus: base64::decode(Alphabet::Url, N.as_bytes(), 1024).expect("vector"),
        exponent: boxed(&[1, 0, 1]),
    };
    let signature = base64::decode(Alphabet::Url, SIGNATURE.as_bytes(), 1024).expect("vector");
    let mut flipped = signature.clone();
    *flipped.first_mut().expect("vector") ^= 1;
    let split = bytes::find(INPUT, b".").expect("vector");
    let header = base64::decode(Alphabet::Url, INPUT.get(..split).expect("vector"), 1024).expect("vector");
    let payload =
        base64::decode(Alphabet::Url, INPUT.get(split.checked_add(1).expect("small")..).expect("vector"), 1024)
            .expect("vector");
    for (name, sig, expected) in
        [("vector", signature.as_ref(), Check::Issuer), ("flipped", flipped.as_ref(), Check::Signature)]
    {
        let token = jws(&header, &payload, sig);
        let got = jwt::verify_id_token(
            &token,
            core::slice::from_ref(&key),
            &id_expected(),
            wall(1000),
            Duration::ZERO,
            &limits(),
        );
        assert_eq!(got.err(), Some(expected), "{name}");
    }
}
