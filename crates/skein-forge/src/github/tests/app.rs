use crate::api;
use crate::github::{guid, jwt, redirect, setup};
use alloc::boxed::Box;
use skein_json::writer;
use skein_lib::{Wall, base64};

const ISSUED: u64 = 1_792_483_200_000_000_000;
const EXPIRES: u64 = 1_792_483_800_000_000_000;
const INPUT: &[u8] =
    b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpYXQiOjE3OTI0ODMyMDAsImV4cCI6MTc5MjQ4MzgwMCwiaXNzIjoiMTIzNDU2In0";

#[test]
fn github_jwt_signing_input() {
    // 2026-10-20T08:00:00Z and ten minutes later.
    const CLAIMS: &[u8] = b"{\"iat\":1792483200,\"exp\":1792483800,\"iss\":\"123456\"}";
    let expected_claims = INPUT.get(37..).unwrap();
    assert_eq!(base64::decode(base64::Alphabet::Url, expected_claims, 128).unwrap().as_ref(), CLAIMS);
    assert_eq!(jwt::LIFE.as_nanos(), 600_000_000_000);
    for (label, issued, expires) in [
        ("whole seconds", ISSUED, EXPIRES),
        ("issued fraction", ISSUED.saturating_add(999_999_999), EXPIRES),
        ("both fractions", ISSUED.saturating_add(123), EXPIRES.saturating_add(123)),
    ] {
        assert_eq!(
            jwt::signing_input(123_456, Wall::from_nanos(issued), Wall::from_nanos(expires)).unwrap().as_ref(),
            INPUT,
            "{label}"
        );
    }
    for (label, issued, expires) in [
        ("equal", ISSUED, ISSUED),
        ("before", ISSUED, ISSUED.saturating_sub(1)),
        ("601 seconds", ISSUED, EXPIRES.saturating_add(1_000_000_000)),
        ("600 seconds and a nanosecond", ISSUED, EXPIRES.saturating_add(1)),
    ] {
        assert_eq!(jwt::signing_input(123_456, Wall::from_nanos(issued), Wall::from_nanos(expires)), None, "{label}");
    }
    // Wall's unsigned epoch offset excludes pre-epoch inputs by construction.
    assert!(jwt::signing_input(0, Wall::EPOCH, Wall::from_nanos(1_000_000_000)).is_some());
    let last = Wall::from_nanos(u64::MAX);
    let first = Wall::from_nanos(u64::MAX.saturating_sub(jwt::LIFE.as_nanos()));
    let input = jwt::signing_input(u64::MAX, first, last).unwrap();
    assert_eq!(input.as_ref(), b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpYXQiOjE4NDQ2NzQzNDczLCJleHAiOjE4NDQ2NzQ0MDczLCJpc3MiOiIxODQ0Njc0NDA3MzcwOTU1MTYxNSJ9");
    assert!(jwt::token(&input, &[0xab; 256]).unwrap().len() <= jwt::JWT_BYTES);
}

#[test]
fn github_jwt_token() {
    const EXPECTED: &[u8] = concat!(
        "input.",
        "q6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6ur",
        "q6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6ur",
        "q6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6ur",
        "q6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6ur",
        "q6urq6urq6urq6urq6urqw"
    )
    .as_bytes();
    let token = jwt::token(b"input", &[0xab; 256]).unwrap();
    assert_eq!(token.expose(), EXPECTED);
    assert_eq!(token.len(), 348);
    assert_eq!(alloc::fmt::format(format_args!("{token:?}")), "Secret { len: 348 }");
    let input = [b'x'; 170];
    assert_eq!(jwt::token(&input, &[0xab; 256]), None);
    let boundary = jwt::token(input.get(..169).unwrap(), &[0xab; 256]).unwrap();
    assert_eq!(boundary.len(), jwt::JWT_BYTES);
}

#[test]
fn github_redirect_parse() {
    type Row = (&'static str, &'static [u8], bool, &'static [u8], u16, &'static [u8]);
    let rows: [Row; 9] = [
        ("TLS", b"https://h/p?q", true, b"h", 443, b"/p?q"),
        ("plain", b"http://127.0.0.1:8080/x", false, b"127.0.0.1", 8080, b"/x"),
        ("no path", b"https://H.EXAMPLE", true, b"H.EXAMPLE", 443, b"/"),
        ("fragment", b"https://h/p?q#fragment", true, b"h", 443, b"/p?q"),
        ("IPv6", b"http://[::1]:9/x", false, b"[::1]", 9, b"/x"),
        ("query only", b"https://h?q", true, b"h", 443, b"/?q"),
        ("fragment only", b"https://h#fragment", true, b"h", 443, b"/"),
        ("plain default", b"http://h", false, b"h", 80, b"/"),
        ("IPv6 default", b"https://[::1]/", true, b"[::1]", 443, b"/"),
    ];
    for (label, location, tls, host, port, target) in rows {
        assert_eq!(
            redirect::parse(location, 64, 64),
            Ok(redirect::Redirect {
                origin: redirect::Origin { tls, host: Box::from(host), port },
                target: Box::from(target),
            }),
            "{label}"
        );
    }
    let invalid: [(&str, &[u8]); 18] = [
        ("other scheme", b"ftp://h/x"),
        ("no scheme", b"//h/x"),
        ("relative", b"/x"),
        ("userinfo", b"https://user@h/x"),
        ("empty userinfo", b"https://@h/x"),
        ("empty host", b"https:///x"),
        ("empty host with port", b"https://:443/x"),
        ("port overflow", b"https://h:65536/x"),
        ("nondigit port", b"https://h:a/x"),
        ("signed port", b"https://h:-1/x"),
        ("empty port", b"https://h:/x"),
        ("unclosed IPv6", b"http://[::1/x"),
        ("unbracketed IPv6", b"http://::1/x"),
        ("empty IPv6", b"http://[]/x"),
        ("bracket suffix", b"http://[::1]oops/x"),
        ("space", b"https://h/a b"),
        ("line break", b"https://h/\r\nx"),
        ("backslash", b"https://h\\other/x"),
    ];
    for (label, location) in invalid {
        assert_eq!(redirect::parse(location, 64, 64), Err(api::Error::InvalidAnswer), "{label}");
    }
    for (label, location, host_bytes, target_bytes) in [
        ("host bound", b"https://host/p".as_slice(), 3, 2),
        ("target bound", b"https://host/p", 4, 1),
        ("implicit path bound", b"https://h?q", 1, 2),
        ("empty target bound", b"https://h", 1, 0),
    ] {
        assert_eq!(redirect::parse(location, host_bytes, target_bytes), Err(api::Error::InvalidAnswer), "{label}");
    }
    assert_eq!(redirect::parse(b"https://host/p#ignored", 4, 2).unwrap().target.as_ref(), b"/p");
    let origin = redirect::Origin { tls: true, host: Box::from(*b"H.Example"), port: 443 };
    assert!(origin.same(&redirect::Origin { host: Box::from(*b"h.example"), ..origin.clone() }));
    assert!(!origin.same(&redirect::Origin { tls: false, ..origin.clone() }));
    assert!(!origin.same(&redirect::Origin { port: 80, ..origin.clone() }));
    assert!(!origin.same(&redirect::Origin { host: Box::from(*b"other"), ..origin.clone() }));
}

#[test]
fn github_guid() {
    let text = b"00112233-4455-6677-8899-aabbccddeeff";
    let expected = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
    assert_eq!(guid::read(text), Some(expected));
    assert_eq!(guid::write(&expected), *text);
    assert_eq!(guid::read(b"00112233-4455-6677-8899-AABBCCDDEEFF"), Some(expected));
    for (label, text) in [
        ("short", b"00112233-4455-6677-8899-aabbccddeef".as_slice()),
        ("long", b"00112233-4455-6677-8899-aabbccddeeff0"),
        ("moved hyphen", b"0011223-34455-6677-8899-aabbccddeeff"),
        ("missing hyphen", b"0011223304455-6677-8899-aabbccddeeff"),
        ("nonhex", b"00112233-4455-6677-8899-aabbccddeefg"),
        ("extra hyphen", b"00112233-4455-6677-8899-aabbccddeef-"),
    ] {
        assert_eq!(guid::read(text), None, "{label}");
    }
}

fn values() -> setup::Values<'static> {
    setup::Values {
        name: b"app",
        url: b"https://service",
        hook_url: b"https://app.invalid/",
        redirect_url: b"http://127.0.0.1/redirect",
        setup_url: b"http://127.0.0.1/setup",
    }
}

#[test]
fn github_setup_manifest() {
    let limits = writer::Limits { depth: 2, length: 2048 };
    let expected = br#"{"name":"app","url":"https://service","public":true,"redirect_url":"http://127.0.0.1/redirect","setup_url":"http://127.0.0.1/setup","setup_on_update":true,"hook_attributes":{"url":"https://app.invalid/","active":true},"default_permissions":{"contents":"write","pull_requests":"write","issues":"write","merge_queues":"write","checks":"read","statuses":"read","actions":"read","administration":"read","metadata":"read"},"default_events":["issues","issue_comment","pull_request","pull_request_review","pull_request_review_comment","push","create","delete","check_run","check_suite","status","workflow_job","workflow_run","merge_group"]}"#;
    assert_eq!(setup::manifest(&values(), &limits).unwrap().as_ref(), expected);
    assert_eq!(
        setup::PERMISSIONS,
        [
            (b"contents".as_slice(), setup::Access::Write),
            (b"pull_requests", setup::Access::Write),
            (b"issues", setup::Access::Write),
            (b"merge_queues", setup::Access::Write),
            (b"checks", setup::Access::Read),
            (b"statuses", setup::Access::Read),
            (b"actions", setup::Access::Read),
            (b"administration", setup::Access::Read),
            (b"metadata", setup::Access::Read),
        ]
    );
    assert_eq!(
        setup::EVENTS,
        [
            b"issues".as_slice(),
            b"issue_comment",
            b"pull_request",
            b"pull_request_review",
            b"pull_request_review_comment",
            b"push",
            b"create",
            b"delete",
            b"check_run",
            b"check_suite",
            b"status",
            b"workflow_job",
            b"workflow_run",
            b"merge_group"
        ]
    );
    let length = u32::try_from(expected.len()).unwrap();
    assert_eq!(
        setup::manifest(&values(), &writer::Limits { length: length.saturating_sub(1), ..limits }),
        Err(writer::Refusal::TooLong)
    );
    assert_eq!(setup::manifest(&values(), &writer::Limits { length, ..limits }).unwrap().as_ref(), expected);
    assert_eq!(setup::manifest(&values(), &writer::Limits { depth: 1, ..limits }), Err(writer::Refusal::TooDeep));
    assert_eq!(setup::manifest(&setup::Values { name: &[255], ..values() }, &limits), Err(writer::Refusal::Text));
    let escaped = setup::manifest(&setup::Values { name: b"\"\\\n", ..values() }, &limits).unwrap();
    assert!(escaped.starts_with(br#"{"name":"\"\\\n","url":"#));
}

#[test]
fn github_setup_pages() {
    let web = b"https://github.com";
    let state = b"Ab09._~-";
    assert_eq!(
        setup::form_target(web, &setup::Account::Person, state, 1024).unwrap().as_ref(),
        b"https://github.com/settings/apps/new?state=Ab09._~-"
    );
    assert_eq!(
        setup::form_target(web, &setup::Account::Organization(b"org-name"), state, 1024).unwrap().as_ref(),
        b"https://github.com/organizations/org-name/settings/apps/new?state=Ab09._~-"
    );
    assert_eq!(
        setup::install_page(web, b"app-slug", state, 1024).unwrap().as_ref(),
        b"https://github.com/apps/app-slug/installations/new?state=Ab09._~-"
    );
    assert_eq!(
        setup::form_target(b"http://127.0.0.1:8080", &setup::Account::Person, b"s", 1024).unwrap().as_ref(),
        b"http://127.0.0.1:8080/settings/apps/new?state=s"
    );
    let manifest = br#"{"value":"&<>'"}"#;
    let expected = b"<!doctype html><meta charset=\"utf-8\"><title>GitHub App</title><form method=\"post\" action=\"https://h/?a=&quot;&amp;&lt;&gt;&#39;\"><input type=\"hidden\" name=\"manifest\" value=\"{&quot;value&quot;:&quot;&amp;&lt;&gt;&#39;&quot;}\"></form><script>document.forms[0].submit()</script>";
    let target = b"https://h/?a=\"&<>'";
    assert_eq!(setup::form_page(target, manifest, 1024).unwrap().as_ref(), expected);
    let length = u32::try_from(expected.len()).unwrap();
    assert!(setup::form_page(target, manifest, length).is_some());
    assert!(setup::form_page(target, manifest, length.saturating_sub(1)).is_none());
    for (label, web, state) in [
        ("trailing slash", b"https://github.com/".as_slice(), b"s".as_slice()),
        ("state space", web, b"a b"),
        ("empty state", web, b""),
        ("empty origin", b"", b"s"),
        ("state separator", web, b"s&x=1"),
    ] {
        assert!(setup::form_target(web, &setup::Account::Person, state, 1024).is_none(), "{label}");
        assert!(setup::form_target(web, &setup::Account::Organization(b"org"), state, 1024).is_none(), "{label}");
        assert!(setup::install_page(web, b"app", state, 1024).is_none(), "{label}");
    }
    for (label, segment) in [("empty", b"".as_slice()), ("space", b"a b"), ("slash", b"a/b"), ("non-ASCII", &[255])] {
        assert!(setup::form_target(web, &setup::Account::Organization(segment), state, 1024).is_none(), "{label}");
        assert!(setup::install_page(web, segment, state, 1024).is_none(), "{label}");
    }
    for account in [setup::Account::Person, setup::Account::Organization(b"org")] {
        let target = setup::form_target(web, &account, state, 1024).unwrap();
        let length = u32::try_from(target.len()).unwrap();
        assert!(setup::form_target(web, &account, state, length).is_some());
        assert!(setup::form_target(web, &account, state, length.saturating_sub(1)).is_none());
    }
    let install = setup::install_page(web, b"slug", state, 1024).unwrap();
    let length = u32::try_from(install.len()).unwrap();
    assert!(setup::install_page(web, b"slug", state, length).is_some());
    assert!(setup::install_page(web, b"slug", state, length.saturating_sub(1)).is_none());
}
