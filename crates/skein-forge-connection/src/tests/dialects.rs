//! Both dialects over one bounded HTTP connection, with no socket or held credential.
use super::fixtures::{self, ConnectionHarness};
use crate::connection::{self, Exchange, Phase};
use alloc::boxed::Box;
use skein_forge::github::{
    credential,
    document::{self, Document, Row},
    header,
    plans::{Body, Failure, Heard},
    request::{Request, Scope},
};
use skein_lib::{
    Secret, Wall, Writer,
    stream::{Fault, Up},
};

const APP_TOKEN: &[u8] = b"artificial-app-jwt";
const INSTALLATION_TOKEN: &[u8] = b"artificial-installation-token";
const INSTALLATION: u64 = 7;
const ACCOUNT: u64 = 22;
const BOT: u64 = 9;
const REPOSITORY: u64 = 4;
const PAGE: u32 = 1;
// The fixture's Date and document times are 2020-01-01T00:00:00Z.
const DATE: Wall = Wall::from_nanos(1_577_836_800_000_000_000);
const INSTALLATION_BODY: &[u8] = br#"{"id":7,"account":{"id":22,"login":"acme","type":"Organization"},"created_at":"2020-01-01T00:00:00Z","suspended_at":null}"#;
const VIEWER_BODY: &[u8] = br#"{"data":{"viewer":{"databaseId":9,"login":"test-app[bot]"}}}"#;
const ERROR_BODY: &[u8] = br#"{"message":"Resource not accessible by integration","errors":[{"message":"detail"}]}"#;
// A response head precedes each terminal; this listing also emits two rows.
const HEAD_AND_DONE: u32 = 2;
const HEAD_ROWS_DONE: u32 = 4;
// The raw boundary fixture keeps three bytes, and tests one byte past it.
const RAW_CAP: u32 = 3;

fn raw_request() -> Request {
    Request::Raw {
        owner: Box::from(b"acme".as_slice()),
        name: Box::from(b"tools".as_slice()),
        path: Box::from(b"dir/file".as_slice()),
        commit: [0; 32],
    }
}
fn start(h: &mut ConnectionHarness, request: Request, token: Option<&[u8]>) {
    #[expect(clippy::manual_map, reason = "the workspace bans Option::map and closures in step tests")]
    let authorization = match token {
        Some(token) => Some(credential::bearer(&Secret::new(Box::from(token))).expect("artificial token")),
        None => None,
    };
    h.connection
        .start(
            &h.env,
            &h.forge,
            fixtures::USER_AGENT,
            Exchange::GitHub {
                exchange: skein_forge::github::plans::plan::Exchange {
                    request,
                    retention: None,
                    comparison: skein_forge::github::plans::plan::Comparison::Full,
                },
                authorization,
            },
        )
        .expect("valid GitHub request");
}
fn begun(request: Request) -> ConnectionHarness {
    let mut h = ConnectionHarness::github();
    start(&mut h, request, Some(INSTALLATION_TOKEN));
    h
}
fn expected_head(status: u16) -> Heard {
    let mut head = header::Head::new();
    head.date = Some(DATE);
    Heard::Head { status, head }
}
fn installation() -> document::Installation {
    document::Installation {
        id: INSTALLATION,
        login: Box::from(b"acme".as_slice()),
        account: ACCOUNT,
        kind: document::Account::Organization,
        created: DATE,
        suspended: false,
    }
}

#[test]
fn github_requests_carry_their_method_target_pin_and_bearer() {
    let rows: [(&str, Request, &[u8], &[u8]); 3] = [
        ("installations on App JWT", Request::Installations { page: PAGE }, APP_TOKEN,
            b"GET /app/installations?per_page=100&page=1 HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nUser-Agent: kit/0.0.0\r\nAccept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2026-03-10\r\nAuthorization: Bearer artificial-app-jwt\r\n\r\n"),
        ("viewer on installation token", Request::Viewer, INSTALLATION_TOKEN,
            b"POST /graphql HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nUser-Agent: kit/0.0.0\r\nAccept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2026-03-10\r\nAuthorization: Bearer artificial-installation-token\r\nContent-Type: application/json\r\nContent-Length: 49\r\n\r\n{\"query\":\"query { viewer { login databaseId } }\"}"),
        ("raw representation", raw_request(), INSTALLATION_TOKEN,
            b"GET /repos/acme/tools/contents/dir/file?ref=0000000000000000000000000000000000000000 HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nUser-Agent: kit/0.0.0\r\nAccept: application/vnd.github.raw+json\r\nX-GitHub-Api-Version: 2026-03-10\r\nAuthorization: Bearer artificial-installation-token\r\n\r\n"),
    ];
    for (label, request, token, expected) in rows {
        let mut h = ConnectionHarness::github();
        start(&mut h, request, Some(token));
        h.github_answer(&fixtures::response(401, ERROR_BODY));
        assert_eq!(h.sent.as_ref(), expected, "{label}: whole request and sole Authorization");
    }
}

#[test]
fn github_requests_without_auth_send_no_authorization() {
    let mut h = ConnectionHarness::github();
    start(&mut h, Request::Conversion { code: Box::from(b"fixture-code".as_slice()) }, None);
    h.github_answer(&fixtures::response(401, ERROR_BODY));
    assert_eq!(h.sent.as_ref(), b"POST /app-manifests/fixture-code/conversions HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nUser-Agent: kit/0.0.0\r\nAccept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2026-03-10\r\nContent-Length: 0\r\n\r\n", "unauthenticated conversion");
}

#[test]
fn github_successes_use_the_requests_decoder() {
    let rows: [(&str, Request, u16, &[u8], Body); 5] = [
        ("installation", Request::Installation { id: INSTALLATION }, 200, INSTALLATION_BODY,
            Body::Document(Document::Installation(installation()))),
        ("minted", Request::Mint { installation: INSTALLATION, scope: Scope::Full }, 201,
            br#"{"token":"artificial-minted-token","expires_at":"2020-01-01T01:00:00Z"}"#,
            Body::Document(Document::Minted(document::Minted {
                token: Secret::new(Box::from(b"artificial-minted-token".as_slice())),
                expires: Wall::from_nanos(1_577_840_400_000_000_000),
            }))),
        ("viewer", Request::Viewer, 200, VIEWER_BODY,
            Body::Document(Document::Viewer(document::Viewer { id: BOT, login: Box::from(b"test-app[bot]".as_slice()) }))),
        ("repository", Request::Repository { id: REPOSITORY }, 200,
            br#"{"id":4,"owner":{"id":22,"login":"acme","type":"Organization"},"name":"tools","default_branch":"main","html_url":"https://forge.test/acme/tools","allow_merge_commit":true,"allow_squash_merge":false,"allow_rebase_merge":true}"#,
            Body::Document(Document::Repository(document::Repository {
                id: REPOSITORY, owner: Box::from(b"acme".as_slice()), owner_id: ACCOUNT, organization: true,
                name: Box::from(b"tools".as_slice()), default_branch: Box::from(b"main".as_slice()),
                html_url: Box::from(b"https://forge.test/acme/tools".as_slice()), merge: true, squash: false, rebase: true,
            }))),
        ("raw", raw_request(), 200, b"raw bytes",
            Body::Raw { bytes: Box::from(b"raw bytes".as_slice()), more: false }),
    ];
    for (label, request, status, body, expected) in rows {
        let mut h = begun(request);
        let heard = h.github_answer(&fixtures::response(status, body));
        assert_eq!(heard.len(), HEAD_AND_DONE, "{label}: head then body");
        assert_eq!(heard.get(0), Some(&expected_head(status)), "{label}: head");
        assert_eq!(heard.get(1), Some(&Heard::Done(expected)), "{label}: selected decoder");
        assert_eq!(h.connection.phase, Phase::Idle, "{label}: reusable");
    }
    let mut h = begun(Request::Installations { page: PAGE });
    let pieces: [&[u8]; 5] = [b"[", INSTALLATION_BODY, b",", INSTALLATION_BODY, b"]"];
    let mut length = 0_usize;
    for piece in pieces {
        length = length.checked_add(piece.len()).expect("listing length");
    }
    let mut body = Writer::new(length);
    for piece in pieces {
        body.put(piece).expect("measured listing");
    }
    let heard = h.github_answer(&fixtures::ok(&body.finish()));
    assert_eq!(heard.len(), HEAD_ROWS_DONE, "streamed rows between head and done");
    assert_eq!(
        heard.as_slice(),
        [
            expected_head(200),
            Heard::Row(Row::Installation(installation())),
            Heard::Row(Row::Installation(installation())),
            Heard::Done(Body::Document(Document::Rows))
        ],
        "row demand retained through the next row and document end"
    );
    assert_eq!(h.connection.phase, Phase::Idle, "streamed document ended");
}

#[test]
fn github_empty_answers_are_discarded() {
    let mut h = ConnectionHarness::github();
    start(&mut h, Request::Uninstall { id: INSTALLATION }, Some(APP_TOKEN));
    let heard = h.github_answer(&fixtures::response(204, b""));
    assert_eq!(heard.as_slice(), [expected_head(204), Heard::Done(Body::Empty)]);
    assert_eq!(h.connection.phase, Phase::Idle);
}

#[test]
fn github_error_statuses_use_the_error_decoder() {
    for (label, status) in [
        ("redirect", 301),
        ("unauthorized", 401),
        ("forbidden", 403),
        ("missing", 404),
        ("rate limited", 429),
        ("server failure", 500),
    ] {
        let mut h = begun(Request::Viewer);
        let heard = h.github_answer(&fixtures::response(status, ERROR_BODY));
        assert_eq!(
            heard.as_slice(),
            [
                expected_head(status),
                Heard::Done(Body::Document(Document::Error(document::Refusal {
                    message: Box::from(b"Resource not accessible by integration".as_slice()),
                    errors: Box::from([Box::from(b"detail".as_slice())]),
                })))
            ],
            "{label}: head before refusal document"
        );
    }
}

#[test]
fn github_heads_keep_rate_and_next_fields() {
    let mut h = begun(Request::Uninstall { id: INSTALLATION });
    let heard = h.github_answer(b"HTTP/1.1 204 No Content\r\nx-ratelimit-reset: 1577836860\r\nRetry-After: Wed, 01 Jan 2020 00:00:30 GMT\r\nx-ratelimit-remaining: 42\r\nLink: <https://forge.test/app/installations?cursor=next%2Bpage>; rel=\"next\"\r\nETag: W/\"fixture\"\r\nLocation: /next\r\nContent-Type: application/json; charset=utf-8\r\nx-ratelimit-resource: graphql\r\nDate: Wed, 01 Jan 2020 00:00:00 GMT\r\nx-ratelimit-limit: 5000\r\nConnection: close\r\n\r\n");
    let Some(Heard::Head { status, head }) = heard.get(0) else {
        unreachable!("head first");
    };
    assert_eq!(*status, 204, "status beside head");
    assert_eq!(head.date, Some(DATE), "Date");
    assert_eq!(head.retry_after, Some(skein_lib::Duration::from_secs(30)), "retry date before Date");
    assert!(head.next, "Link next");
    assert_eq!(head.cursor, header::Cursor::new(b"next+page"), "next cursor");
    assert_eq!(head.etag, header::Etag::new(b"W/\"fixture\""), "validator");
    assert_eq!(head.location.as_deref(), Some(b"/next".as_slice()), "location");
    assert!(head.json, "JSON media type");
    assert_eq!(
        head.rate,
        Some(header::RateFields { limit: 5000, remaining: 42, reset: Wall::from_nanos(1_577_836_860_000_000_000) }),
        "complete rate window"
    );
    assert_eq!(head.resource, Some(header::Resource::Graphql), "rate bucket");
    assert!(head.close, "close field");
    assert_eq!(heard.get(1), Some(&Heard::Done(Body::Empty)), "head precedes body");
}

#[test]
fn github_incomplete_rate_fields_leave_no_window() {
    for (label, response, remaining) in [
        ("only remaining", b"HTTP/1.1 204 No Content\r\nx-ratelimit-remaining: 42\r\n\r\n".as_slice(), Some(42)),
        (
            "only limit and reset",
            b"HTTP/1.1 204 No Content\r\nx-ratelimit-limit: 5000\r\nx-ratelimit-reset: 1577836860\r\n\r\n",
            None,
        ),
    ] {
        let mut h = begun(Request::Uninstall { id: INSTALLATION });
        let heard = h.github_answer(response);
        let Some(Heard::Head { head, .. }) = heard.get(0) else {
            unreachable!("head first");
        };
        assert_eq!(head.rate, None, "{label}: missing companion");
        assert_eq!(head.remaining(), remaining, "{label}: independent remaining field");
    }
}

#[derive(Clone, Copy, Debug)]
enum FaultAt {
    Reset,
    Deadline,
}
#[test]
fn github_faults_before_and_after_a_send_have_visibility() {
    for (label, send, expected) in [("before send", false, Failure::Unseen), ("after send", true, Failure::Seen)] {
        for fault in [FaultAt::Reset, FaultAt::Deadline] {
            let mut h = begun(Request::Viewer);
            h.pump();
            if send {
                h.connection.stream(&h.env, Up::Room);
                h.pump();
            }
            match fault {
                FaultAt::Reset => h.connection.stream(&h.env, Up::Failed(Fault::Reset)),
                FaultAt::Deadline => h.connection.timeout(),
            }
            h.pump();
            assert_eq!(
                h.connection.heard(),
                Some(connection::Heard::GitHub(Heard::Failed(expected))),
                "{label}/{fault:?}"
            );
            h.connection.timeout();
            h.connection.stream(&h.env, Up::Failed(Fault::Reset));
            h.pump();
            assert_eq!(h.connection.heard(), None, "{label}/{fault:?}: one terminal");
            assert_eq!(h.connection.phase, Phase::Closed, "{label}/{fault:?}: closed");
        }
    }
}

#[test]
fn github_stream_failure_during_json_is_seen() {
    let mut h = begun(Request::Viewer);
    let heard = h.github_answer(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{\"data\":");
    assert_eq!(heard.len(), 1, "only the head before the stream fails");
    h.connection.stream(&h.env, Up::Failed(Fault::Reset));
    h.pump();
    assert_eq!(
        h.connection.heard(),
        Some(connection::Heard::GitHub(Heard::Failed(Failure::Seen))),
        "stream failure retains visibility"
    );
    assert_eq!(h.connection.heard(), None, "one failure");
    assert_eq!(h.connection.phase, Phase::Closed, "failed body closes");
}

#[test]
fn github_connection_close_finishes_before_closing() {
    let mut h = begun(Request::Uninstall { id: INSTALLATION });
    let heard = h.github_answer(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
    assert_eq!(heard.last(), Some(&Heard::Done(Body::Empty)), "answer completes");
    assert_eq!(h.connection.phase, Phase::Closed, "no reusable connection");
    assert_eq!(h.connection.deadline, None, "no idle deadline");
}

#[test]
fn github_malformed_json_closes_after_its_head() {
    let mut h = begun(Request::Viewer);
    let heard = h.github_answer(&fixtures::ok(b"{"));
    assert_eq!(heard.len(), HEAD_AND_DONE, "head then malformed");
    assert_eq!(heard.get(0), Some(&expected_head(200)), "head preserved");
    let Some(Heard::Done(Body::Malformed(_))) = heard.last() else {
        unreachable!("malformed terminal");
    };
    assert_eq!(h.connection.phase, Phase::Closed, "no reusable half-read body");
    assert_eq!(h.connection.deadline, None, "no idle deadline");
}

#[test]
fn github_empty_refusal_keeps_its_status_before_malformed() {
    let mut h = begun(Request::Viewer);
    let heard = h.github_answer(&fixtures::response(403, b""));
    assert_eq!(heard.len(), HEAD_AND_DONE, "head then malformed refusal");
    assert_eq!(heard.get(0), Some(&expected_head(403)), "status preserved");
    let Some(Heard::Done(Body::Malformed(_))) = heard.last() else {
        unreachable!("empty refusal is malformed JSON");
    };
    assert_eq!(h.connection.phase, Phase::Closed, "malformed refusal closes");
}

#[test]
fn github_raw_overflow_closes_with_a_bounded_prefix() {
    for (label, body, more, phase) in
        [("at cap", b"abc".as_slice(), false, Phase::Idle), ("past cap", b"abcd", true, Phase::Closed)]
    {
        let mut h = ConnectionHarness::github_with(crate::Shared { answer_bytes: RAW_CAP, ..fixtures::shared() });
        start(&mut h, raw_request(), Some(INSTALLATION_TOKEN));
        let heard = h.github_answer(&fixtures::ok(body));
        assert_eq!(
            heard.last(),
            Some(&Heard::Done(Body::Raw { bytes: Box::from(b"abc".as_slice()), more })),
            "{label}: bounded prefix"
        );
        assert_eq!(h.connection.phase, phase, "{label}: no half-consumed reuse");
    }
}
