//! Task 18's documents and plans are driven entirely in process.
use super::fixtures::{self, bytes, decode, limits};
use crate::{
    api,
    github::{
        decode::Malformed,
        document::{self, Document, Kind, Row},
        header::Head,
        plans::{
            Body, Failure, Heard,
            plan::{Bounds, Plan, Step},
            tables::{self, Tables},
        },
        request::{self, Accept, Request},
        server,
    },
};
use alloc::boxed::Box;
use skein_lib::Writer;

const KEY: api::Repository = api::Repository { forge: 3, repository: 7 };
const USER: u64 = 9_007_199_254_740_993;
const WORD: &[u8] = b"example-app";
const ROOM: u32 = 4096;
fn bounds() -> Bounds {
    Bounds { page_size: 4, rows: 4, answer_bytes: ROOM, body_bytes: 128 }
}
fn owner() -> Box<[u8]> {
    Box::from(*b"acme")
}
fn name() -> Box<[u8]> {
    Box::from(*b"widgets")
}
fn user() -> document::User {
    document::User { id: USER, login: Box::from(*b"alice") }
}
fn permission_request() -> Request {
    Request::Permission { owner: owner(), name: name(), login: Box::from(*b"alice") }
}
struct Call {
    plan: Plan,
    tables: Tables,
    bounds: Bounds,
}
impl Call {
    fn new(read: api::Read, bounds: Bounds, cached: bool) -> Self {
        let mut tables = Tables::new(2, 2);
        if cached {
            tables.keep_repository(
                KEY,
                tables::Repository {
                    owner: owner(),
                    name: name(),
                    owner_id: 8,
                    organization: true,
                    merge: None,
                    html_url: Box::from([]),
                },
            );
        }
        Self { plan: Plan::new(KEY, api::Op::Read(read)), tables, bounds }
    }
    fn begin(&mut self) -> Result<Step, api::Error> {
        self.plan.begin(self.bounds, WORD, &mut self.tables)
    }
    fn heard(&mut self, heard: Heard) -> Result<Option<Step>, api::Error> {
        self.plan.heard(heard, self.bounds, WORD, &mut self.tables)
    }
    fn head(&mut self, next: bool) {
        let mut head = Head::new();
        head.next = next;
        assert_eq!(self.heard(Heard::Head { status: 200, head }), Ok(None));
    }
    fn done(&mut self, document: Document) -> Result<Option<Step>, api::Error> {
        self.heard(Heard::Done(Body::Document(document)))
    }
}
fn started(read: api::Read, bounds: Bounds) -> Call {
    let mut call = Call::new(read, bounds, true);
    let Ok(Step::Send(_)) = call.begin() else {
        panic!("expected request");
    };
    call.head(false);
    call
}
#[expect(clippy::unnecessary_wraps, reason = "expected verdicts retain the plan event result shape")]
fn ended(value: Result<api::Answer, api::Error>) -> Result<Option<Step>, api::Error> {
    Ok(Some(Step::Done(value)))
}
fn golden(request: &Request, target: &[u8], kind: Option<Kind>, accept: Accept) {
    let encoded = request::encode(request, &limits()).unwrap();
    assert_eq!(encoded.target.as_ref(), target);
    assert_eq!(encoded.accept, accept);
    assert_eq!(crate::github::decode::kind(request), kind);
}
fn document(kind: Kind, wire: &[u8], expected: Document, expected_rows: &[Row]) {
    for chunk in [1, 7] {
        let (rows, value) = decode(kind, wire, chunk, &limits());
        assert_eq!(value, Ok(expected.clone()));
        assert_eq!(rows.as_slice(), expected_rows);
    }
}
fn malformed(kind: Kind, wire: &[u8]) {
    for chunk in [1, 7] {
        decode(kind, wire, chunk, &limits()).1.unwrap_err();
    }
}
#[test]
fn github_permission_cached_and_uncached_user() {
    golden(&Request::User { id: USER }, b"/user/9007199254740993", Some(Kind::User), Accept::Json);
    golden(
        &permission_request(),
        b"/repos/acme/widgets/collaborators/alice/permission",
        Some(Kind::Permission),
        Accept::Json,
    );
    for cached in [false, true] {
        let mut call = Call::new(api::Read::Permission { user: USER }, bounds(), true);
        if cached {
            call.tables.keep_user(KEY.forge, USER, user().login);
        }
        let request = if cached { permission_request() } else { Request::User { id: USER } };
        assert_eq!(call.begin(), Ok(Step::send(request)));
        call.head(false);
        if !cached {
            assert_eq!(call.done(Document::User(user())), Ok(Some(Step::send(permission_request()))));
            call.head(false);
        }
        assert_eq!(
            call.done(Document::Permission(api::Permission::Write)),
            ended(Ok(api::Answer::Permission(api::Permission::Write)))
        );
        assert_eq!(call.tables.user(KEY.forge, USER), Some(b"alice".as_slice()));
    }
    let mut call = Call::new(api::Read::Permission { user: USER }, bounds(), false);
    assert_eq!(call.begin(), Ok(Step::send(Request::Repository { id: 7 })));
    call.head(false);
    let mut repository = fixtures::repository();
    repository.id = 7;
    repository.owner = owner();
    repository.name = name();
    assert_eq!(call.done(Document::Repository(repository)), Ok(Some(Step::send(Request::User { id: USER }))));
}
#[test]
fn github_permission_roles_and_bad_user() {
    for (role, expected) in [
        ("admin", api::Permission::Admin),
        ("maintain", api::Permission::Admin),
        ("write", api::Permission::Write),
        ("triage", api::Permission::Read),
        ("read", api::Permission::Read),
        ("none", api::Permission::None),
    ] {
        let mut wire = Writer::new(64);
        wire.put(b"{\"role_name\":\"").unwrap();
        wire.put(role.as_bytes()).unwrap();
        wire.put(b"\"}").unwrap();
        document(Kind::Permission, &bytes(wire), Document::Permission(expected), &[]);
        let mut out = Writer::new(usize::try_from(ROOM).unwrap());
        server::permission(expected, &mut out).unwrap();
        document(Kind::Permission, &bytes(out), Document::Permission(expected), &[]);
    }
    let mut out = Writer::new(usize::try_from(ROOM).unwrap());
    server::user(&user(), &mut out).unwrap();
    document(Kind::User, &bytes(out), Document::User(user()), &[]);
    for (kind, wire) in [
        (Kind::Permission, b"{\"role_name\":\"future\"}".as_slice()),
        (Kind::Permission, b"{}"),
        (Kind::Permission, b"{\"role_name\":false}"),
        (Kind::Permission, b"{\"role_name\":\"read\",\"role_name\":null}"),
        (Kind::User, b"{\"id\":18446744073709551616,\"login\":\"alice\"}"),
        (Kind::User, b"{\"id\":1}"),
    ] {
        malformed(kind, wire);
    }
    let mut call = started(api::Read::Permission { user: USER }, bounds());
    assert_eq!(
        call.done(Document::User(document::User { id: 1, login: user().login })),
        ended(Err(api::Error::InvalidAnswer))
    );
    assert_eq!(call.tables.user(KEY.forge, USER), None);
}
#[test]
fn github_permission_bounds_and_failures() {
    for length in [128, 129] {
        let row = document::User { id: USER, login: Box::from(&[b'x'; 129][..length]) };
        let mut out = Writer::new(usize::try_from(ROOM).unwrap());
        server::user(&row, &mut out).unwrap();
        let wire = bytes(out);
        for chunk in [1, 7] {
            assert_eq!(decode(Kind::User, &wire, chunk, &limits()).1.is_ok(), length == 128);
        }
    }
    let mut small = Writer::new(1);
    assert_eq!(server::user(&user(), &mut small), Err(skein_lib::Overflow));
    assert_eq!(small.written(), 0);
    for (status, error) in [
        (404, api::Error::Missing),
        (410, api::Error::Missing),
        (401, api::Error::Forbidden),
        (500, api::Error::Timeout),
    ] {
        let mut call = Call::new(api::Read::Permission { user: USER }, bounds(), true);
        call.begin().unwrap();
        assert_eq!(call.heard(Heard::Head { status, head: Head::new() }), ended(Err(error)));
    }
    for (failure, error) in [(Failure::Unseen, api::Error::Unavailable), (Failure::Seen, api::Error::Timeout)] {
        let mut call = started(api::Read::Permission { user: USER }, bounds());
        assert_eq!(call.heard(Heard::Failed(failure)), ended(Err(error)));
    }
    let mut call = started(api::Read::Permission { user: USER }, bounds());
    assert_eq!(
        call.heard(Heard::Done(Body::Malformed(Malformed::Value { field: b"role_name" }))),
        ended(Err(api::Error::InvalidAnswer))
    );
}

fn collaborator(id: u64, permission: api::Permission) -> document::Collaborator {
    document::Collaborator { id, permission, login: None }
}
fn collaborator_row(call: &mut Call, row: document::Collaborator) -> Result<Option<Step>, api::Error> {
    call.heard(Heard::Row(Row::Collaborator(row)))
}
#[test]
fn github_collaborators_roles_without_fanout() {
    let request = Request::Collaborators { owner: owner(), name: name(), page: 3, per_page: 4 };
    golden(
        &request,
        b"/repos/acme/widgets/collaborators?affiliation=all&per_page=4&page=3",
        Some(Kind::CollaboratorRows),
        Accept::Json,
    );
    let rows = [
        collaborator(USER, api::Permission::Admin),
        collaborator(2, api::Permission::Write),
        collaborator(3, api::Permission::Read),
        collaborator(4, api::Permission::None),
    ];
    let mut out = Writer::new(usize::try_from(ROOM).unwrap());
    server::collaborators(&rows, &mut out).unwrap();
    let expected = rows.clone().map(Row::Collaborator);
    document(Kind::CollaboratorRows, &bytes(out), Document::Rows, &expected);
    let mut call = Call::new(api::Read::Collaborators { page: 3 }, bounds(), true);
    assert_eq!(call.begin(), Ok(Step::send(request)));
    call.head(false);
    for row in rows {
        assert_eq!(collaborator_row(&mut call, row), Ok(None));
    }
    assert_eq!(
        call.done(Document::Rows),
        ended(Ok(api::Answer::Collaborators {
            more: false,
            collaborators: Box::from([
                api::Collaborator { user: USER, permission: api::Permission::Admin },
                api::Collaborator { user: 2, permission: api::Permission::Write },
                api::Collaborator { user: 3, permission: api::Permission::Read },
                api::Collaborator { user: 4, permission: api::Permission::None }
            ])
        }))
    );
}
#[test]
fn github_collaborators_link_and_bounds() {
    for more in [false, true] {
        let mut call =
            Call::new(api::Read::Collaborators { page: 1 }, Bounds { rows: 1, answer_bytes: 16, ..bounds() }, true);
        call.begin().unwrap();
        call.head(more);
        let mut row = collaborator(USER, api::Permission::Read);
        row.login = Some(user().login);
        assert_eq!(collaborator_row(&mut call, row), Ok(None));
        assert_eq!(call.tables.user(KEY.forge, USER), Some(b"alice".as_slice()));
        assert_eq!(
            call.done(Document::Rows),
            ended(Ok(api::Answer::Collaborators {
                more,
                collaborators: Box::from([api::Collaborator { user: USER, permission: api::Permission::Read }])
            }))
        );
    }
    let mut call = started(api::Read::Collaborators { page: 1 }, Bounds { rows: 1, ..bounds() });
    collaborator_row(&mut call, collaborator(USER, api::Permission::Read)).unwrap();
    assert_eq!(collaborator_row(&mut call, collaborator(2, api::Permission::Read)), ended(Err(api::Error::TooLarge)));
    for bounds in
        [Bounds { answer_bytes: 15, ..bounds() }, Bounds { rows: 0, ..bounds() }, Bounds { page_size: 0, ..bounds() }]
    {
        assert_eq!(Call::new(api::Read::Collaborators { page: 1 }, bounds, true).begin(), Err(api::Error::TooLarge));
    }
}
#[test]
fn github_collaborators_malformed_and_failures() {
    for wire in [
        br#"[{"id":1,"role_name":"future"}]"#.as_slice(),
        br#"[{"role_name":"read"}]"#,
        br#"[{"id":1,"id":null,"role_name":"read"}]"#,
        br#"[{"id":1,"role_name":true}]"#,
    ] {
        malformed(Kind::CollaboratorRows, wire);
    }
    for chunk in [1, 7] {
        let (rows, value) = decode(
            Kind::CollaboratorRows,
            br#"[{"id":1,"login":"alice","role_name":"maintain"},{"id":2,"role_name":"triage"}]"#,
            chunk,
            &limits(),
        );
        assert_eq!(value, Ok(Document::Rows));
        assert_eq!(
            rows.as_slice(),
            &[
                Row::Collaborator(document::Collaborator {
                    id: 1,
                    login: Some(user().login),
                    permission: api::Permission::Admin
                }),
                Row::Collaborator(collaborator(2, api::Permission::Read))
            ]
        );
    }
    let mut call = started(api::Read::Collaborators { page: 1 }, bounds());
    assert_eq!(call.done(Document::Permission(api::Permission::Read)), ended(Err(api::Error::InvalidAnswer)));
    let mut call = Call::new(api::Read::Collaborators { page: 1 }, bounds(), true);
    call.begin().unwrap();
    assert_eq!(call.heard(Heard::Head { status: 404, head: Head::new() }), ended(Err(api::Error::Missing)));
}

fn remark(id: u64) -> document::Remark {
    document::Remark {
        id,
        author: USER,
        path: Box::from(*b"dir/x y"),
        line: 0,
        body: crate::forgejo::document::Body::Kept(Box::from(*b"hello")),
    }
}
fn remarks_read() -> api::Read {
    api::Read::Remarks { number: 3, review: 4, page: 2 }
}
#[test]
fn github_remarks_line_fallbacks() {
    golden(
        &Request::ReviewComments { owner: owner(), name: name(), number: 3, review: 4, page: 2, per_page: 4 },
        b"/repos/acme/widgets/pulls/3/reviews/4/comments?per_page=4&page=2",
        Some(Kind::RemarkRows),
        Accept::Json,
    );
    for (lines, expected) in [
        (b"\"line\":2,\"original_line\":3".as_slice(), 2),
        (b"\"line\":null,\"original_line\":3", 3),
        (b"\"line\":0,\"original_line\":3", 0),
        (b"\"original_line\":4294967295", u32::MAX),
        (b"\"line\":null", 0),
    ] {
        let mut wire = Writer::new(512);
        wire.put(br#"[{"id":1,"user":{"id":9007199254740993},"path":"dir/x y","body":"hello","#).unwrap();
        wire.put(lines).unwrap();
        wire.put(b"}]").unwrap();
        let mut row = remark(1);
        row.line = expected;
        document(Kind::RemarkRows, &bytes(wire), Document::Rows, &[Row::Remark(row)]);
    }
}
#[test]
fn github_remarks_page_and_long_body() {
    let mut row = remark(1);
    row.body = crate::forgejo::document::Body::Kept(Box::from([b'x'; 129]));
    let mut out = Writer::new(usize::try_from(ROOM).unwrap());
    server::remarks(&[row], &mut out).unwrap();
    let mut long = remark(1);
    long.body = crate::forgejo::document::Body::Long(129);
    document(Kind::RemarkRows, &bytes(out), Document::Rows, &[Row::Remark(long.clone())]);
    for more in [false, true] {
        let mut call = Call::new(remarks_read(), bounds(), true);
        call.begin().unwrap();
        call.head(more);
        assert_eq!(call.heard(Heard::Row(Row::Remark(long.clone()))), Ok(None));
        assert_eq!(
            call.done(Document::Rows),
            ended(Ok(api::Answer::Remarks {
                more,
                remarks: Box::from([api::Remark {
                    id: 1,
                    author: USER,
                    path: remark(1).path,
                    line: 0,
                    body: Box::from([])
                }])
            }))
        );
    }
    document(
        Kind::RemarkRows,
        br#"[{"id":1,"user":{"id":9007199254740993},"path":"dir/x y","body":null}]"#,
        Document::Rows,
        &[Row::Remark(document::Remark { body: crate::forgejo::document::Body::Kept(Box::from([])), ..remark(1) })],
    );
}
#[test]
fn github_remarks_bounds_and_failures() {
    for wire in [
        br#"[{"id":1,"user":{"id":1},"path":"x","line":4294967296}]"#.as_slice(),
        br#"[{"id":1,"user":{"id":1},"path":"x","original_line":4294967296}]"#,
        br#"[{"id":1,"user":{"id":1},"path":"x","line":1,"line":null}]"#,
        br#"[{"id":1,"path":"x"}]"#,
        br#"[{"id":1,"user":{"id":1},"path":"x","body":false}]"#,
    ] {
        malformed(Kind::RemarkRows, wire);
    }
    for (room, expected) in [(76, Ok(None)), (75, ended(Err(api::Error::TooLarge)))] {
        let mut call = started(remarks_read(), Bounds { answer_bytes: room, ..bounds() });
        assert_eq!(call.heard(Heard::Row(Row::Remark(remark(1)))), expected);
    }
    let mut call = started(remarks_read(), Bounds { page_size: 1, ..bounds() });
    call.heard(Heard::Row(Row::Remark(remark(1)))).unwrap();
    assert_eq!(call.heard(Heard::Row(Row::Remark(remark(2)))), ended(Err(api::Error::TooLarge)));
    let mut call = started(remarks_read(), bounds());
    assert_eq!(call.done(Document::User(user())), ended(Err(api::Error::InvalidAnswer)));
    let mut call = Call::new(remarks_read(), bounds(), true);
    call.begin().unwrap();
    assert_eq!(call.heard(Heard::Head { status: 404, head: Head::new() }), ended(Err(api::Error::Missing)));
}

fn review(id: u64) -> document::Review {
    document::Review {
        id,
        author: USER,
        verdict: Some(api::Verdict::Approve),
        commit: crate::github::id::commit(fixtures::SHA).unwrap(),
        body: crate::forgejo::document::Body::Kept(Box::from(*b"hello")),
        submitted: Some(fixtures::wall()),
        official: true,
    }
}
fn reviews_read() -> api::Read {
    api::Read::Reviews { number: 3, page: 2 }
}
fn review_answer(call: &mut Call) -> (Box<[api::Review]>, bool) {
    let Ok(Some(Step::Done(Ok(api::Answer::Reviews { reviews, more })))) = call.done(Document::Rows) else {
        panic!("Reviews answer");
    };
    (reviews, more)
}
#[test]
fn github_reviews_verdicts_pending_and_official() {
    golden(
        &Request::Reviews { owner: owner(), name: name(), number: 3, page: 2, per_page: 4 },
        b"/repos/acme/widgets/pulls/3/reviews?per_page=4&page=2",
        Some(Kind::ReviewRows),
        Accept::Json,
    );
    for (state, verdict) in [
        (b"APPROVED".as_slice(), Some(api::Verdict::Approve)),
        (b"CHANGES_REQUESTED", Some(api::Verdict::RequestChanges)),
        (b"COMMENTED", Some(api::Verdict::Comment)),
        (b"DISMISSED", Some(api::Verdict::Comment)),
        (b"PENDING", None),
    ] {
        for (association, official) in [
            (b"OWNER".as_slice(), true),
            (b"MEMBER", true),
            (b"COLLABORATOR", true),
            (b"CONTRIBUTOR", false),
            (b"future", false),
        ] {
            let mut wire = Writer::new(512);
            wire.put(br#"[{"id":1,"user":{"id":9007199254740993},"state":""#).unwrap();
            wire.put(state).unwrap();
            wire.put(br#"","commit_id":"0123456789abcdef0123456789abcdef01234567","body":"hello","submitted_at":"2026-10-20T08:00:00Z","author_association":""#).unwrap();
            wire.put(association).unwrap();
            wire.put(b"\"}]").unwrap();
            let row = document::Review { verdict, official, ..review(1) };
            document(Kind::ReviewRows, &bytes(wire), Document::Rows, &[Row::Review(row)]);
        }
    }
    let mut call = started(reviews_read(), bounds());
    for row in [
        review(1),
        document::Review { verdict: None, ..review(2) },
        document::Review { submitted: None, ..review(3) },
        document::Review { verdict: Some(api::Verdict::RequestChanges), ..review(4) },
    ] {
        call.heard(Heard::Row(Row::Review(row))).unwrap();
    }
    let (rows, more) = review_answer(&mut call);
    assert!(!more);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, 1);
    assert_eq!(rows[1].id, 4);
    assert_eq!(rows[1].verdict, api::Verdict::RequestChanges);
    assert_eq!(rows[0].at.as_nanos(), fixtures::wall().as_nanos());
}
#[test]
fn github_reviews_marker_revision_and_long_body() {
    let marked = crate::marker::encode(WORD, b"key", b"hello", 128).unwrap();
    let mut call = started(reviews_read(), bounds());
    let rows = [
        document::Review { body: crate::forgejo::document::Body::Kept(marked.clone()), ..review(1) },
        document::Review { body: crate::forgejo::document::Body::Long(129), ..review(2) },
    ];
    for row in rows {
        call.heard(Heard::Row(Row::Review(row))).unwrap();
    }
    let (rows, _) = review_answer(&mut call);
    assert_eq!(rows[0].body, marked);
    assert_eq!(rows[0].key, Some(Box::from(*b"key")));
    assert_eq!(rows[0].provenance, api::Provenance::Original);
    assert_eq!(rows[0].revision, u64::from_be_bytes(skein_lib::sha256::digest(&marked)[..8].try_into().unwrap()));
    assert_eq!(rows[1].body.as_ref(), b"");
    assert_eq!(rows[1].key, None);
    assert_eq!(rows[1].provenance, api::Provenance::Unknown);
    let mut fallback = Writer::new(16);
    fallback.put(&129u64.to_be_bytes()).unwrap();
    fallback.put(&0u64.to_be_bytes()).unwrap();
    assert_eq!(
        rows[1].revision,
        u64::from_be_bytes(skein_lib::sha256::digest(&fallback.finish())[..8].try_into().unwrap())
    );
    for length in [128, 129] {
        let row = document::Review {
            body: crate::forgejo::document::Body::Kept(Box::from(&[b'x'; 129][..length])),
            ..review(1)
        };
        let mut out = Writer::new(usize::try_from(ROOM).unwrap());
        server::reviews(core::slice::from_ref(&row), &mut out).unwrap();
        let expected = if length == 128 {
            row
        } else {
            document::Review { body: crate::forgejo::document::Body::Long(129), ..review(1) }
        };
        document(Kind::ReviewRows, &bytes(out), Document::Rows, &[Row::Review(expected)]);
    }
}
#[test]
fn github_reviews_page_bounds_and_failures() {
    for more in [false, true] {
        let mut call = Call::new(reviews_read(), Bounds { answer_bytes: 133, ..bounds() }, true);
        call.begin().unwrap();
        call.head(more);
        call.heard(Heard::Row(Row::Review(review(1)))).unwrap();
        assert_eq!(review_answer(&mut call).1, more);
    }
    let mut call = started(reviews_read(), Bounds { answer_bytes: 132, ..bounds() });
    assert_eq!(call.heard(Heard::Row(Row::Review(review(1)))), ended(Err(api::Error::TooLarge)));
    let mut call = started(reviews_read(), Bounds { page_size: 1, ..bounds() });
    call.heard(Heard::Row(Row::Review(document::Review { verdict: None, ..review(1) }))).unwrap();
    assert_eq!(call.heard(Heard::Row(Row::Review(review(2)))), ended(Err(api::Error::TooLarge)));
    let mut call = started(reviews_read(), bounds());
    call.heard(Heard::Row(Row::Review(review(2)))).unwrap();
    assert_eq!(call.heard(Heard::Row(Row::Review(review(1)))), ended(Err(api::Error::InvalidAnswer)));
    for field in [
        b"\"state\":\"future\"".as_slice(),
        b"\"state\":false",
        b"\"state\":\"APPROVED\",\"state\":null",
        b"\"commit_id\":\"A\"",
    ] {
        let mut wire = Writer::new(512);
        wire.put(b"[{").unwrap();
        wire.put(field).unwrap();
        wire.put(b"}]").unwrap();
        malformed(Kind::ReviewRows, &bytes(wire));
    }
    let mut call = Call::new(reviews_read(), bounds(), true);
    call.begin().unwrap();
    assert_eq!(call.heard(Heard::Head { status: 404, head: Head::new() }), ended(Err(api::Error::Missing)));
}

fn file_read(max_bytes: u32) -> api::Read {
    api::Read::File { head: review(1).commit, path: Box::from(*b"dir/x y"), max_bytes }
}
fn raw_body(call: &mut Call, bytes: &[u8], more: bool) -> Result<Option<Step>, api::Error> {
    call.heard(Heard::Done(Body::Raw { bytes: Box::from(bytes), more }))
}
fn raw_request() -> Request {
    Request::Raw { owner: owner(), name: name(), path: Box::from(*b"dir/x y"), commit: review(1).commit }
}
#[test]
fn github_file_raw_target_and_prefix() {
    golden(
        &raw_request(),
        b"/repos/acme/widgets/contents/dir/x%20y?ref=0123456789abcdef0123456789abcdef01234567",
        None,
        Accept::Raw,
    );
    let mut call = Call::new(file_read(3), bounds(), true);
    assert_eq!(
        call.begin(),
        Ok(Step::Send(crate::github::plans::plan::Exchange {
            request: raw_request(),
            retention: Some(crate::github::plans::plan::Retention::Head { bytes: 3 })
        }))
    );
    call.head(false);
    assert_eq!(
        raw_body(&mut call, b"abc", true),
        ended(Ok(api::Answer::File {
            head: review(1).commit,
            path: Box::from(*b"dir/x y"),
            bytes: Box::from(*b"abc"),
            truncated: true
        }))
    );
}
#[test]
fn github_file_directory_and_missing() {
    let mut call = Call::new(file_read(3), bounds(), true);
    call.begin().unwrap();
    let mut head = Head::new();
    head.json = true;
    assert_eq!(call.heard(Heard::Head { status: 200, head }), ended(Err(api::Error::Missing)));
    for status in [404, 410] {
        let mut call = Call::new(file_read(3), bounds(), true);
        call.begin().unwrap();
        assert_eq!(call.heard(Heard::Head { status, head: Head::new() }), ended(Err(api::Error::Missing)));
    }
    let mut call = started(file_read(3), bounds());
    assert_eq!(call.done(Document::Rows), ended(Err(api::Error::InvalidAnswer)));
    let mut call = started(file_read(3), bounds());
    assert_eq!(call.heard(Heard::Failed(Failure::Seen)), ended(Err(api::Error::Timeout)));
}
#[test]
fn github_file_exact_limit_and_truncated() {
    for (room, max_bytes, kept, more) in [
        (10, 8, b"abc".as_slice(), true),
        (10, 3, b"abc", false),
        (7, 3, b"", true),
        (7, 0, b"", false),
        (8, 1, b"a", false),
    ] {
        let mut call = Call::new(file_read(max_bytes), Bounds { answer_bytes: room, ..bounds() }, true);
        let cap = max_bytes.min(room - 7);
        assert_eq!(
            call.begin(),
            Ok(Step::Send(crate::github::plans::plan::Exchange {
                request: raw_request(),
                retention: Some(crate::github::plans::plan::Retention::Head { bytes: cap })
            }))
        );
        call.head(false);
        assert_eq!(
            raw_body(&mut call, kept, more),
            ended(Ok(api::Answer::File {
                head: review(1).commit,
                path: Box::from(*b"dir/x y"),
                bytes: Box::from(kept),
                truncated: more
            }))
        );
    }
    assert_eq!(
        Call::new(file_read(3), Bounds { answer_bytes: 6, ..bounds() }, true).begin(),
        Err(api::Error::TooLarge)
    );
    let mut call = started(file_read(3), bounds());
    assert_eq!(raw_body(&mut call, b"abcd", false), ended(Err(api::Error::TooLarge)));
}
