use crate::{Outgoing, Settings, Token, http, instance::ini};
use std::path::Path;
#[test]
fn the_app_ini_names_loopback_sqlite_and_the_page_cap() {
    let rendered = ini(Path::new("/scratch"), 3000, &Settings::small(), "fixture", "secret", "internal");
    assert_eq!(
        rendered,
        "APP_NAME = kit throwaway\nRUN_USER = fixture\nWORK_PATH = /scratch\n[server]\nPROTOCOL = http\nHTTP_ADDR = 127.0.0.1\nHTTP_PORT = 3000\nROOT_URL = http://127.0.0.1:3000/\nAPP_DATA_PATH = /scratch/data\nDISABLE_SSH = true\nOFFLINE_MODE = true\n[database]\nDB_TYPE = sqlite3\nPATH = /scratch/forgejo.db\n[repository]\nROOT = /scratch/repositories\nDEFAULT_BRANCH = main\n[security]\nINSTALL_LOCK = true\nSECRET_KEY = secret\nINTERNAL_TOKEN = internal\n[service]\nDISABLE_REGISTRATION = true\n[log]\nMODE = console\nLEVEL = Error\n[git]\nPATH = /usr/bin/git\n[actions]\nENABLED = true\n[api]\nMAX_RESPONSE_ITEMS = 2\n"
    );
}
#[test]
fn a_chunked_answer_is_read_whole_and_unframed() {
    let response = http::parse(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ntest\r\n3;x=y\r\ning\r\n0\r\n\r\n",
        Vec::new(),
    )
    .expect("valid chunks");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"testing");
}
#[test]
fn a_recorded_request_holds_no_token() {
    // The user transcript's head, with its 554-byte body excluded.
    const HEAD: &[u8] = b"HTTP/1.1 200 OK\r\nCache-Control: max-age=0, private, must-revalidate\r\nContent-Type: application/json;charset=utf-8\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: SAMEORIGIN\r\nDate: Wed, 01 Jan 2020 00:00:00 GMT\r\nConnection: close\r\nContent-Length: 554\r\n";
    const BODY_BYTES: usize = 554;

    let token = Token::new("0123456789abcdef0123456789abcdef01234567".into());
    let outgoing = Outgoing::get("/api/v1/user", Some(&token));
    let sent = http::request(3000, &outgoing, Some("token MASKED"));
    assert_eq!(sent, b"GET /api/v1/user HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nAuthorization: token MASKED\r\nConnection: close\r\n\r\n");
    assert_eq!(format!("{token:?}"), "Token { length: 40 }");
    let raw = [HEAD, b"\r\n", &[b' '; BODY_BYTES]].concat();
    let response = http::parse(&raw, sent.clone()).expect("recorded user answer");
    assert_eq!(response.sent, sent);
    assert_eq!(response.head, HEAD);
}

#[test]
fn a_range_is_sent_as_a_head_or_a_suffix() {
    use crate::Range;
    for (range, line) in [(Range::Tail(10), "Range: bytes=-10\r\n"), (Range::Head(3), "Range: bytes=0-3\r\n")] {
        let outgoing = Outgoing { range: Some(range), ..Outgoing::get("/api/v1/version", None) };
        let sent = String::from_utf8(http::request(3000, &outgoing, None)).expect("UTF-8 request");
        assert!(sent.contains(line), "{range:?}: {sent}");
    }
}

#[test]
fn an_output_shows_its_lengths_and_no_text() {
    let output = crate::Output { code: Some(0), stdout: "secret-token\n".into(), stderr: String::new() };
    assert_eq!(format!("{output:?}"), "Output { code: Some(0), stdout_bytes: 13, stderr_bytes: 0 }");
}

#[test]
fn the_app_ini_allows_the_webhook_host_it_is_given() {
    let mut settings = Settings::small();
    settings.webhook_hosts = Some("127.0.0.1".into());
    let rendered = ini(Path::new("/scratch"), 3000, &settings, "fixture", "secret", "internal");
    assert!(rendered.ends_with("[webhook]\nALLOWED_HOST_LIST = 127.0.0.1\n"));
}
#[test]
fn a_fresh_secret_is_64_hexadecimal_digits_and_never_repeats() {
    let a = crate::fresh_secret().expect("random source");
    let b = crate::fresh_secret().expect("random source");
    assert_eq!(a.len(), 64);
    assert_eq!(b.len(), 64);
    assert!(a.bytes().all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)));
    assert!(b.bytes().all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)));
    assert_ne!(a, b);
}
#[test]
fn a_basic_request_records_its_scheme_and_holds_no_credential() {
    let sent = http::request(3000, &Outgoing::get("/api/v1/user", None), Some("Basic MASKED"));
    assert_eq!(sent, b"GET /api/v1/user HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nAuthorization: Basic MASKED\r\nConnection: close\r\n\r\n");
}
