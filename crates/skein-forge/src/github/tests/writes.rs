use super::fixtures::{decode, limits};
use crate::{
    api,
    github::{
        decode as decoding,
        document::{self, Document, Kind},
        header::Head,
        plans::{
            Body, Failure, Heard,
            plan::{Bounds, Plan, Step},
            tables::{self, Tables},
        },
        request::{self, Auth, Method, Request},
    },
};
use alloc::boxed::Box;
const KEY: api::Repository = api::Repository { forge: 1, repository: 7 };
const WORD: &[u8] = b"example-app";
const NUMBER: u64 = 9_007_199_254_740_993;
fn repository() -> document::Repository {
    document::Repository {
        id: 7,
        owner: Box::from(*b"acme"),
        owner_id: NUMBER,
        organization: true,
        name: Box::from(*b"widgets"),
        default_branch: Box::from(*b"main"),
        html_url: Box::from([]),
        merge: true,
        squash: false,
        rebase: false,
    }
}
fn bounds() -> Bounds {
    Bounds { page_size: 4, rows: 4, answer_bytes: 4096, body_bytes: 4096 }
}
fn cached_tables() -> Tables {
    let mut tables = Tables::new(1, 1);
    tables.keep_repository(KEY, tables::repository(&repository()));
    tables
}
fn write_plan(write: api::Write) -> Plan {
    Plan::new(KEY, api::Op::Write(write))
}
fn begin(plan: &mut Plan, tables: &mut Tables) -> Request {
    let Step::Send(exchange) = plan.begin(bounds(), WORD, tables).unwrap() else { panic!("expected send") };
    exchange.request
}
fn head(plan: &mut Plan, status: u16, tables: &mut Tables) {
    assert_eq!(plan.heard(Heard::Head { status, head: Head::new() }, bounds(), WORD, tables), Ok(None));
}
fn finish(plan: &mut Plan, body: Body, tables: &mut Tables) -> Result<Option<Step>, api::Error> {
    plan.heard(Heard::Done(body), bounds(), WORD, tables)
}
#[expect(clippy::unnecessary_wraps, reason = "the expected value includes the plan's call-order result")]
fn ended(answer: Result<api::Answer, api::Error>) -> Result<Option<Step>, api::Error> {
    Ok(Some(Step::Done(answer)))
}
fn refusal(message: &[u8]) -> Body {
    Body::Document(Document::Error(document::Refusal { message: Box::from(message), errors: Box::from([]) }))
}
fn golden(request: &Request, method: Method, target: &[u8], body: Option<&[u8]>, kind: Option<Kind>) {
    let encoded = request::encode(request, &limits()).unwrap();
    assert_eq!(encoded.method, method);
    assert_eq!(encoded.target.as_ref(), target);
    assert_eq!(encoded.body.as_deref(), body);
    assert_eq!(encoded.auth, Auth::Installation);
    assert_eq!(
        request::fields(&encoded),
        [
            (b"Accept".as_slice(), b"application/vnd.github+json".as_slice()),
            (b"X-GitHub-Api-Version".as_slice(), b"2026-03-10".as_slice())
        ]
    );
    assert_eq!(decoding::kind(request), kind);
}
fn delete() -> api::Write {
    api::Write::DeleteBranch { branch: Box::from(*b"alice/x y") }
}
#[test]
fn github_delete_branch_request_and_empty() {
    let mut tables = cached_tables();
    let mut plan = write_plan(delete());
    let request = begin(&mut plan, &mut tables);
    golden(&request, Method::Delete, b"/repos/acme/widgets/git/refs/heads/alice/x%20y", None, None);
    let target_len = u32::try_from(request::encode(&request, &limits()).unwrap().target.len()).unwrap();
    for (target_bytes, ok) in [(target_len, true), (target_len - 1, false)] {
        assert_eq!(request::encode(&request, &crate::github::limits::Limits { target_bytes, ..limits() }).is_ok(), ok);
    }
    for (string_bytes, ok) in [(9, true), (8, false)] {
        assert_eq!(request::encode(&request, &crate::github::limits::Limits { string_bytes, ..limits() }).is_ok(), ok);
    }
    head(&mut plan, 204, &mut tables);
    assert_eq!(finish(&mut plan, Body::Empty, &mut tables), ended(Ok(api::Answer::Done)));
    assert_eq!(finish(&mut plan, Body::Empty, &mut tables), Err(api::Error::InvalidAnswer));
}
#[test]
fn github_delete_branch_missing_and_failures() {
    for (status, message, error) in [
        (422, b"Reference does not exist".as_slice(), api::Error::Missing),
        (422, b"unknown", api::Error::Refused),
        (409, b"unknown", api::Error::Refused),
        (403, b"protected", api::Error::Forbidden),
        (500, b"unknown", api::Error::Timeout),
        (301, b"moved", api::Error::Unavailable),
        (404, b"gone", api::Error::Missing),
    ] {
        let mut tables = cached_tables();
        let mut plan = write_plan(delete());
        begin(&mut plan, &mut tables);
        head(&mut plan, status, &mut tables);
        assert_eq!(finish(&mut plan, refusal(message), &mut tables), ended(Err(error)));
        if status == 301 || status == 404 {
            assert_eq!(tables.repository(KEY), None);
        }
        assert_eq!(
            plan.heard(Heard::Failed(Failure::Seen), bounds(), WORD, &mut tables),
            Err(api::Error::InvalidAnswer)
        );
    }
    for body in [
        Body::Document(Document::Rows),
        Body::Malformed(decoding::Malformed::Bound { field: b"body" }),
        Body::Raw { bytes: Box::from([]), more: false },
    ] {
        let mut tables = cached_tables();
        let mut plan = write_plan(delete());
        begin(&mut plan, &mut tables);
        head(&mut plan, 204, &mut tables);
        assert_eq!(finish(&mut plan, body, &mut tables), ended(Err(api::Error::Timeout)));
    }
}
#[test]
fn github_delete_branch_lost_answer_find() {
    let mut tables = cached_tables();
    let mut write = write_plan(delete());
    let sent = begin(&mut write, &mut tables);
    assert_eq!(write.pending(), None);
    let Step::Send(exchange) = Step::send(sent) else { panic!("exchange") };
    write.set_pending(exchange.clone());
    assert_eq!(write.pending(), Some(exchange));
    assert_eq!(write.pending(), None);
    assert_eq!(write.heard(Heard::Failed(Failure::Seen), bounds(), WORD, &mut tables), ended(Err(api::Error::Timeout)));
    let mut find = Plan::new(KEY, api::Op::Read(api::Read::Branch { branch: Box::from(*b"alice/x y") }));
    golden(
        &begin(&mut find, &mut tables),
        Method::Get,
        b"/repos/acme/widgets/branches/alice/x%20y",
        None,
        Some(Kind::Branch),
    );
    assert_eq!(
        find.heard(Heard::Head { status: 404, head: Head::new() }, bounds(), WORD, &mut tables),
        ended(Err(api::Error::Missing))
    );
}
#[test]
fn github_write_resolution_and_first_failure() {
    let mut tables = Tables::new(0, 0);
    let mut write = write_plan(delete());
    assert_eq!(begin(&mut write, &mut tables), Request::Repository { id: 7 });
    head(&mut write, 200, &mut tables);
    assert_eq!(
        finish(&mut write, Body::Document(Document::Repository(repository())), &mut tables),
        Ok(Some(Step::send(Request::DeleteBranch {
            owner: Box::from(*b"acme"),
            name: Box::from(*b"widgets"),
            branch: Box::from(*b"alice/x y")
        })))
    );
    assert_eq!(tables.repository(KEY), None);
    for body in
        [Body::Empty, Body::Document(Document::Rows), Body::Malformed(decoding::Malformed::Shape { field: b"id" })]
    {
        let mut tables = Tables::new(0, 0);
        let mut write = write_plan(delete());
        begin(&mut write, &mut tables);
        head(&mut write, 200, &mut tables);
        assert_eq!(finish(&mut write, body, &mut tables), ended(Err(api::Error::Unavailable)));
    }
    for failed in [Failure::Seen, Failure::Unseen] {
        let mut tables = Tables::new(0, 0);
        let mut write = write_plan(delete());
        begin(&mut write, &mut tables);
        assert_eq!(
            write.heard(Heard::Failed(failed), bounds(), WORD, &mut tables),
            ended(Err(api::Error::Unavailable))
        );
    }
}

fn close() -> api::Write {
    api::Write::Close { number: NUMBER }
}
#[test]
fn github_close_request_and_done() {
    let mut tables = cached_tables();
    let mut plan = write_plan(close());
    let request = begin(&mut plan, &mut tables);
    golden(
        &request,
        Method::Patch,
        b"/repos/acme/widgets/issues/9007199254740993",
        Some(br#"{"state":"closed"}"#),
        Some(Kind::Done),
    );
    let encoded = request::encode(&request, &limits()).unwrap();
    assert_eq!(
        crate::github::server::write_body(request.clone(), encoded.body.as_deref().unwrap(), &limits()),
        Some(request.clone())
    );
    for body in
        [b"{}".as_slice(), br#"{"state":true}"#, br#"{"state":"closed","state":"closed"}"#, br#"{"state":"open"}"#]
    {
        assert_eq!(crate::github::server::write_body(request.clone(), body, &limits()), None);
    }
    let mut out = skein_lib::Writer::new(2);
    crate::github::server::done(&mut out).unwrap();
    assert_eq!(out.finish().as_ref(), b"{}");
    for chunk in [1, 7] {
        assert_eq!(
            decode(Kind::Done, br#"{"unknown":{"nested":[1,"alice"]}}"#, chunk, &limits()).1,
            Ok(Document::Done)
        );
        decode(Kind::Done, b"[]", chunk, &limits()).1.unwrap_err();
    }
    head(&mut plan, 200, &mut tables);
    assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    let body_len = u32::try_from(encoded.body.unwrap().len()).unwrap();
    for (body_bytes, ok) in [(body_len, true), (body_len - 1, false)] {
        assert_eq!(request::encode(&request, &crate::github::limits::Limits { body_bytes, ..limits() }).is_ok(), ok);
    }
}
#[test]
fn github_close_replay_and_failures() {
    for _attempt in 0_u32..2 {
        let mut tables = cached_tables();
        let mut plan = write_plan(close());
        begin(&mut plan, &mut tables);
        head(&mut plan, 200, &mut tables);
        assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    }
    for (status, error) in [(422, api::Error::Refused), (409, api::Error::Refused), (500, api::Error::Timeout)] {
        let mut tables = cached_tables();
        let mut plan = write_plan(close());
        begin(&mut plan, &mut tables);
        head(&mut plan, status, &mut tables);
        assert_eq!(finish(&mut plan, refusal(b"unknown"), &mut tables), ended(Err(error)));
    }
    for body in [Body::Empty, Body::Document(Document::Rows)] {
        let mut tables = cached_tables();
        let mut plan = write_plan(close());
        begin(&mut plan, &mut tables);
        head(&mut plan, 200, &mut tables);
        assert_eq!(finish(&mut plan, body, &mut tables), ended(Err(api::Error::Timeout)));
    }
}

#[test]
fn github_reopen_request_and_done() {
    let mut tables = cached_tables();
    let mut plan = write_plan(api::Write::Reopen { number: NUMBER });
    let request = begin(&mut plan, &mut tables);
    golden(
        &request,
        Method::Patch,
        b"/repos/acme/widgets/issues/9007199254740993",
        Some(br#"{"state":"open"}"#),
        Some(Kind::Done),
    );
    let encoded = request::encode(&request, &limits()).unwrap();
    assert_eq!(
        crate::github::server::write_body(request.clone(), encoded.body.as_deref().unwrap(), &limits()),
        Some(request.clone())
    );
    let size = u32::try_from(encoded.body.unwrap().len()).unwrap();
    for (body_bytes, ok) in [(size, true), (size.saturating_sub(1), false)] {
        assert_eq!(request::encode(&request, &crate::github::limits::Limits { body_bytes, ..limits() }).is_ok(), ok);
    }
    head(&mut plan, 200, &mut tables);
    assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
}
#[test]
fn github_reopen_replay_and_failures() {
    for _attempt in 0_u32..2 {
        let mut tables = cached_tables();
        let mut plan = write_plan(api::Write::Reopen { number: NUMBER });
        begin(&mut plan, &mut tables);
        head(&mut plan, 200, &mut tables);
        assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    }
    for status in [409, 422] {
        let mut tables = cached_tables();
        let mut plan = write_plan(api::Write::Reopen { number: NUMBER });
        begin(&mut plan, &mut tables);
        head(&mut plan, status, &mut tables);
        assert_eq!(finish(&mut plan, refusal(b"unknown"), &mut tables), ended(Err(api::Error::Refused)));
    }
}

#[expect(clippy::manual_map, reason = "test fixtures spell out optional copies")]
fn edit(title: Option<&[u8]>, body: Option<&[u8]>) -> api::Write {
    api::Write::Edit {
        number: NUMBER,
        title: match title {
            Some(text) => Some(Box::from(text)),
            None => None,
        },
        body: match body {
            Some(text) => Some(Box::from(text)),
            None => None,
        },
    }
}
fn encoded_body_bounds(request: &Request) {
    let size = u32::try_from(request::encode(request, &limits()).unwrap().body.unwrap().len()).unwrap();
    for (body_bytes, ok) in [(size, true), (size.saturating_sub(1), false)] {
        assert_eq!(request::encode(request, &crate::github::limits::Limits { body_bytes, ..limits() }).is_ok(), ok);
    }
}
fn roundtrip_body(request: &Request) {
    let encoded = request::encode(request, &limits()).unwrap();
    assert_eq!(
        crate::github::server::write_body(request.clone(), encoded.body.as_deref().unwrap(), &limits()),
        Some(request.clone())
    );
}
#[test]
fn github_edit_optional_fields_and_pull() {
    for (write, body) in [
        (edit(None, None), b"{}".as_slice()),
        (edit(Some(b""), None), br#"{"title":""}"#),
        (edit(None, Some(b"alice")), br#"{"body":"alice"}"#),
        (edit(Some(b"alice"), Some(b"")), br#"{"title":"alice","body":""}"#),
    ] {
        let mut tables = cached_tables();
        let mut plan = write_plan(write);
        let request = begin(&mut plan, &mut tables);
        golden(&request, Method::Patch, b"/repos/acme/widgets/issues/9007199254740993", Some(body), Some(Kind::Done));
        roundtrip_body(&request);
        head(&mut plan, 200, &mut tables);
        assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    }
}
#[test]
fn github_edit_text_and_bounds() {
    let mut tables = cached_tables();
    let mut plan = write_plan(edit(Some(b"alice\"\\\n"), Some("é".as_bytes())));
    let request = begin(&mut plan, &mut tables);
    encoded_body_bounds(&request);
    roundtrip_body(&request);
    for (title, error) in
        [(Box::<[u8]>::from([0xff]), request::Refusal::Text), (Box::from([b'a'; 129]), request::Refusal::TooLong)]
    {
        let mut plan = write_plan(edit(Some(&title), None));
        let request = begin(&mut plan, &mut tables);
        assert_eq!(request::encode(&request, &limits()), Err(error));
    }
    let request = Request::Edit {
        owner: Box::from(*b"acme"),
        name: Box::from(*b"widgets"),
        number: NUMBER,
        title: Some(Box::from([b'a'; 128])),
        body: None,
    };
    let wide = crate::github::limits::Limits { body_bytes: 256, ..limits() };
    request::encode(&request, &wide).unwrap();
    assert_eq!(
        request::encode(&request, &crate::github::limits::Limits { string_bytes: 127, ..wide }),
        Err(request::Refusal::TooLong)
    );
}
#[test]
fn github_edit_empty_and_replay() {
    for (write, error) in [
        (edit(None, None), api::Error::Refused),
        (edit(Some(b""), None), api::Error::Empty),
        (edit(None, Some(b"")), api::Error::Empty),
    ] {
        let mut tables = cached_tables();
        let mut plan = write_plan(write);
        begin(&mut plan, &mut tables);
        head(&mut plan, 422, &mut tables);
        assert_eq!(finish(&mut plan, refusal(b"unknown"), &mut tables), ended(Err(error)));
    }
    for _attempt in 0_u32..2 {
        let mut tables = cached_tables();
        let mut plan = write_plan(edit(None, None));
        begin(&mut plan, &mut tables);
        head(&mut plan, 200, &mut tables);
        assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    }
}
fn commit() -> api::Commit {
    crate::github::id::commit(super::fixtures::SHA).unwrap()
}
fn status(check: api::Check) -> api::Write {
    api::Write::Status { commit: commit(), context: Box::from(*b"alice"), check }
}
#[test]
fn github_status_states_and_request() {
    for (check, body) in [
        (api::Check::Pending, br#"{"state":"pending","context":"alice"}"#.as_slice()),
        (api::Check::Passed, br#"{"state":"success","context":"alice"}"#),
        (api::Check::Failed, br#"{"state":"failure","context":"alice"}"#),
    ] {
        let mut tables = cached_tables();
        let mut plan = write_plan(status(check));
        let request = begin(&mut plan, &mut tables);
        golden(
            &request,
            Method::Post,
            b"/repos/acme/widgets/statuses/0123456789abcdef0123456789abcdef01234567",
            Some(body),
            Some(Kind::Done),
        );
        roundtrip_body(&request);
        head(&mut plan, 201, &mut tables);
        assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    }
}
#[test]
fn github_status_integration_forbidden() {
    let mut tables = cached_tables();
    let mut plan = write_plan(status(api::Check::Pending));
    begin(&mut plan, &mut tables);
    head(&mut plan, 403, &mut tables);
    assert_eq!(
        finish(&mut plan, refusal(crate::github::failure::INTEGRATION), &mut tables),
        ended(Err(api::Error::Forbidden))
    );
}
#[test]
fn github_status_replay_and_bounds() {
    for _attempt in 0_u32..2 {
        let mut tables = cached_tables();
        let mut plan = write_plan(status(api::Check::Passed));
        let request = begin(&mut plan, &mut tables);
        encoded_body_bounds(&request);
        head(&mut plan, 201, &mut tables);
        assert_eq!(finish(&mut plan, Body::Document(Document::Done), &mut tables), ended(Ok(api::Answer::Done)));
    }
    let mut tables = cached_tables();
    let mut plan = write_plan(status(api::Check::Pending));
    let mut request = begin(&mut plan, &mut tables);
    if let Request::Status { commit, .. } = &mut request {
        *commit = [255; 32];
    }
    assert_eq!(request::encode(&request, &limits()), Err(request::Refusal::Text));
    for (context, ok) in [(Box::<[u8]>::from([b'a'; 128]), true), (Box::from([b'a'; 129]), false)] {
        let request = Request::Status {
            owner: Box::from(*b"acme"),
            name: Box::from(*b"widgets"),
            commit: commit(),
            context,
            check: api::Check::Pending,
        };
        assert_eq!(
            request::encode(&request, &crate::github::limits::Limits { body_bytes: 256, ..limits() }).is_ok(),
            ok
        );
    }
}
fn create_branch() -> api::Write {
    api::Write::CreateBranch { branch: Box::from(*b"alice/x y"), commit: commit() }
}
#[test]
fn github_create_branch_ref_and_bounds() {
    let mut tables = cached_tables();
    let mut plan = write_plan(create_branch());
    let request = begin(&mut plan, &mut tables);
    golden(
        &request,
        Method::Post,
        b"/repos/acme/widgets/git/refs",
        Some(br#"{"ref":"refs/heads/alice/x y","sha":"0123456789abcdef0123456789abcdef01234567"}"#),
        Some(Kind::Done),
    );
    roundtrip_body(&request);
    encoded_body_bounds(&request);
    for (string_bytes, ok) in [(9, true), (8, false)] {
        assert_eq!(request::encode(&request, &crate::github::limits::Limits { string_bytes, ..limits() }).is_ok(), ok);
    }
    head(&mut plan, 201, &mut tables);
    assert_eq!(
        finish(&mut plan, Body::Document(Document::Done), &mut tables),
        ended(Ok(api::Answer::Branch(api::BranchCreation::Created)))
    );
    let request = Request::CreateBranch {
        owner: Box::from(*b"acme"),
        name: Box::from(*b"widgets"),
        branch: Box::from(*b"alice"),
        commit: [255; 32],
    };
    assert_eq!(request::encode(&request, &limits()), Err(request::Refusal::Text));
}
#[test]
fn github_create_branch_exists_not_409() {
    for (status, message, answer) in [
        (422, b"Reference already exists".as_slice(), Ok(api::Answer::Branch(api::BranchCreation::Exists))),
        (409, b"Reference already exists", Err(api::Error::Refused)),
        (422, b"unknown", Err(api::Error::Refused)),
    ] {
        let mut tables = cached_tables();
        let mut plan = write_plan(create_branch());
        begin(&mut plan, &mut tables);
        head(&mut plan, status, &mut tables);
        assert_eq!(finish(&mut plan, refusal(message), &mut tables), ended(answer));
    }
}
#[test]
fn github_create_branch_find() {
    let mut tables = cached_tables();
    let mut write = write_plan(create_branch());
    begin(&mut write, &mut tables);
    assert_eq!(write.heard(Heard::Failed(Failure::Seen), bounds(), WORD, &mut tables), ended(Err(api::Error::Timeout)));
    let mut find = Plan::new(KEY, api::Op::Read(api::Read::Branch { branch: Box::from(*b"alice/x y") }));
    begin(&mut find, &mut tables);
    head(&mut find, 200, &mut tables);
    assert_eq!(
        finish(
            &mut find,
            Body::Document(Document::Branch(document::Branch { name: Box::from(*b"alice/x y"), commit: commit() })),
            &mut tables
        ),
        ended(Ok(api::Answer::Commit(commit())))
    );
}
fn open_pull(title: &[u8], body: &[u8]) -> api::Write {
    api::Write::OpenPull {
        title: Box::from(title),
        body: Box::from(body),
        head: Box::from(*b"alice/x"),
        base: Box::from(*b"main"),
    }
}
fn numeric_result(kind: Kind, body: &[u8], expected: Document) {
    for chunk in [1, 7] {
        assert_eq!(decode(kind, body, chunk, &limits()).1, Ok(expected.clone()));
    }
}
#[test]
fn github_open_pull_single_post_and_number() {
    let mut tables = cached_tables();
    let mut plan = write_plan(open_pull(b"alice", b"text"));
    let request = begin(&mut plan, &mut tables);
    golden(
        &request,
        Method::Post,
        b"/repos/acme/widgets/pulls",
        Some(br#"{"title":"alice","body":"text","head":"alice/x","base":"main"}"#),
        Some(Kind::Number),
    );
    roundtrip_body(&request);
    encoded_body_bounds(&request);
    numeric_result(Kind::Number, br#"{"number":9007199254740993,"unknown":{"nested":[1]}}"#, Document::Number(NUMBER));
    let mut out = skein_lib::Writer::new(br#"{"number":9007199254740993}"#.len());
    crate::github::server::number(NUMBER, &mut out).unwrap();
    numeric_result(Kind::Number, &out.finish(), Document::Number(NUMBER));
    head(&mut plan, 201, &mut tables);
    assert_eq!(
        finish(&mut plan, Body::Document(Document::Number(NUMBER)), &mut tables),
        ended(Ok(api::Answer::Created(NUMBER)))
    );
    for body in
        [b"{}".as_slice(), br#"{"number":1,"number":2}"#, br#"{"number":"1"}"#, br#"{"number":18446744073709551616}"#]
    {
        for chunk in [1, 7] {
            decode(Kind::Number, body, chunk, &limits()).1.unwrap_err();
        }
    }
    for body in [Body::Document(Document::Done), Body::Malformed(decoding::Malformed::Bound { field: b"number" })] {
        let mut tables = cached_tables();
        let mut plan = write_plan(open_pull(b"alice", b"text"));
        begin(&mut plan, &mut tables);
        head(&mut plan, 201, &mut tables);
        assert_eq!(finish(&mut plan, body, &mut tables), ended(Err(api::Error::Timeout)));
    }
}
#[test]
fn github_open_pull_422_neighbours() {
    for (status, message, error) in [
        (422, b"No commits between main and alice".as_slice(), api::Error::NothingToMerge),
        (422, b"A pull request already exists for alice", api::Error::Exists),
        (409, b"A pull request already exists for alice", api::Error::Refused),
        (422, b"unknown", api::Error::Empty),
    ] {
        let mut tables = cached_tables();
        let mut plan = write_plan(open_pull(b"", b""));
        begin(&mut plan, &mut tables);
        head(&mut plan, status, &mut tables);
        assert_eq!(finish(&mut plan, refusal(message), &mut tables), ended(Err(error)));
    }
}
#[test]
fn github_open_pull_lost_find() {
    let mut tables = cached_tables();
    let mut write = write_plan(open_pull(b"alice", b"text"));
    begin(&mut write, &mut tables);
    assert_eq!(write.heard(Heard::Failed(Failure::Seen), bounds(), WORD, &mut tables), ended(Err(api::Error::Timeout)));
    let mut find =
        Plan::new(KEY, api::Op::Read(api::Read::PullFor { head: Box::from(*b"alice/x"), base: Box::from(*b"main") }));
    let request = begin(&mut find, &mut tables);
    golden(
        &request,
        Method::Get,
        b"/repos/acme/widgets/pulls?state=open&head=acme%3Aalice%2Fx&base=main&per_page=1",
        None,
        Some(Kind::PullRows),
    );
    head(&mut find, 200, &mut tables);
    assert_eq!(
        finish(&mut find, Body::Document(Document::Rows), &mut tables),
        Ok(Some(Step::send(Request::PullsFor {
            owner: Box::from(*b"acme"),
            name: Box::from(*b"widgets"),
            head: Box::from(*b"alice/x"),
            base: Box::from(*b"main"),
            open: false
        })))
    );
}

fn create_issue(body: &[u8]) -> api::Write {
    api::Write::CreateIssue { key: Box::from([0, 255]), title: Box::from(*b"alice"), body: Box::from(body) }
}
#[test]
fn github_create_issue_marker_and_number() {
    let mut tables = cached_tables();
    let mut plan = write_plan(create_issue(b"text\n\""));
    let request = begin(&mut plan, &mut tables);
    golden(
        &request,
        Method::Post,
        b"/repos/acme/widgets/issues",
        Some(br#"{"title":"alice","body":"<!-- example-app:key 00ff -->\ntext\n\""}"#),
        Some(Kind::Number),
    );
    roundtrip_body(&request);
    encoded_body_bounds(&request);
    head(&mut plan, 201, &mut tables);
    assert_eq!(
        finish(&mut plan, Body::Document(Document::Number(NUMBER)), &mut tables),
        ended(Ok(api::Answer::Created(NUMBER)))
    );
    for source in [b"".as_slice(), b"text"] {
        let mut plan = write_plan(create_issue(source));
        begin(&mut plan, &mut tables);
        head(&mut plan, 422, &mut tables);
        assert_eq!(
            finish(&mut plan, refusal(b"unknown"), &mut tables),
            ended(Err(if source.is_empty() { api::Error::Empty } else { api::Error::Refused }))
        );
    }
}
#[test]
fn github_create_issue_marker_bounds() {
    let size = crate::marker::length(WORD, 2).unwrap() + 1;
    for (body_bytes, ok) in [(size, true), (size - 1, false)] {
        let mut tables = cached_tables();
        let mut plan = write_plan(create_issue(b"\""));
        let result = plan.begin(Bounds { body_bytes, ..bounds() }, WORD, &mut tables);
        assert_eq!(result.is_ok(), ok);
        if let Ok(Step::Send(exchange)) = result {
            encoded_body_bounds(&exchange.request);
        }
    }
    let mut tables = cached_tables();
    let mut plan = write_plan(create_issue(b""));
    let request = begin(&mut plan, &mut tables);
    assert!(
        crate::github::server::write_body(request.clone(), br#"{"title":"alice","body":"x","body":"y"}"#, &limits())
            .is_none()
    );
    for body in [b"{}".as_slice(), br#"{"title":1,"body":"x"}"#, br#"{"title":"alice"}"#] {
        assert!(crate::github::server::write_body(request.clone(), body, &limits()).is_none());
    }
    head(&mut plan, 201, &mut tables);
    assert_eq!(finish(&mut plan, Body::Empty, &mut tables), ended(Err(api::Error::Timeout)));
}
#[test]
fn github_create_issue_lost_find() {
    let mut tables = cached_tables();
    let mut write = write_plan(create_issue(b"text"));
    begin(&mut write, &mut tables);
    assert_eq!(write.heard(Heard::Failed(Failure::Seen), bounds(), WORD, &mut tables), ended(Err(api::Error::Timeout)));
    for (word, writer, found) in [(WORD, NUMBER, true), (b"alice".as_slice(), NUMBER, false), (WORD, 1, false)] {
        let mut find = Plan::new(
            KEY,
            api::Op::Read(api::Read::Items { since: skein_lib::Time::ZERO, page: 1, kind: Some(api::Kind::Issue) }),
        );
        let request = begin(&mut find, &mut tables);
        assert_eq!(decoding::kind(&request), Some(Kind::IssueRows));
        let mut response = Head::new();
        response.date = Some(super::fixtures::wall());
        assert_eq!(find.heard(Heard::Head { status: 200, head: response }, bounds(), word, &mut tables), Ok(None));
        let (rows, result) = decode(Kind::IssueRows, br#"[{"number":9007199254740993,"state":"open","user":{"id":9007199254740993},"updated_at":"2026-10-20T08:00:00Z","title":"alice","body":"<!-- example-app:key 00ff -->\ntext"}]"#, 1, &limits());
        for row in &rows {
            assert_eq!(find.heard(Heard::Row(row.clone()), bounds(), word, &mut tables), Ok(None));
        }
        let answer = find.heard(Heard::Done(Body::Document(result.unwrap())), bounds(), word, &mut tables).unwrap();
        let Some(Step::Done(Ok(api::Answer::Items { items, more, .. }))) = answer else { panic!("items") };
        assert!(!more);
        let mut observed = false;
        for item in &items {
            observed |= item.key.as_deref() == Some(&[0, 255]) && item.author == writer;
        }
        assert_eq!(observed, found);
    }
}
