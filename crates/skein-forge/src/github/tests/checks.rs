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
        request::{self, Request},
        server,
    },
    measure,
};
use skein_lib::{Time, Wall, Writer};

const KEY: api::Repository = api::Repository { forge: 3, repository: 7 };
fn bounds() -> Bounds {
    Bounds { inner: 8, page_size: 2, rows: 16, answer_bytes: 4096, body_bytes: 1024 }
}
fn head() -> api::Commit {
    fixtures::pull(5).head_commit
}
fn run(id: u64, check: api::Check, app: &[u8]) -> document::Run {
    document::Run {
        id: Some(id),
        head: Some(head()),
        name: Box::from(*b"build"),
        title: Box::from(*b"widgets"),
        url: Box::from(*b"https://example.invalid/check"),
        app: Some(Box::from(app)),
        completed: Some(fixtures::wall()),
        ..document::Run::new(check)
    }
}
fn job(id: u64) -> document::Job {
    document::Job { id, run: 9_007_199_254_740_993, attempt: 2, head: head() }
}
fn request(stage: Kind, page: u32, per_page: u32) -> Request {
    let owner = Box::from(*b"acme");
    let name = Box::from(*b"widgets");
    if stage == Kind::CombinedStatus {
        Request::CombinedStatus { owner, name, commit: head(), page, per_page }
    } else {
        Request::CheckRuns { owner, name, commit: head(), page, per_page }
    }
}
fn job_request(id: u64) -> Request {
    Request::Job { owner: Box::from(*b"acme"), name: Box::from(*b"widgets"), id }
}
struct Call {
    plan: Plan,
    tables: Tables,
    bounds: Bounds,
    chunk: u32,
}
impl Call {
    fn new(bounds: Bounds, cached: bool, chunk: u32) -> Self {
        let mut tables = Tables::new(2, 2);
        if cached {
            tables.keep_repository(
                KEY,
                tables::Repository {
                    owner: Box::from(*b"acme"),
                    name: Box::from(*b"widgets"),
                    owner_id: 8,
                    merge: Some(tables::Method::Merge),
                    html_url: Box::from(*b"https://example.invalid/acme/widgets"),
                    organization: true,
                },
            );
        }
        Self { plan: Plan::new(KEY, api::Op::Read(api::Read::Checks { commit: head() })), tables, bounds, chunk }
    }
    fn begin(&mut self) -> Result<Step, api::Error> {
        self.plan.begin(self.bounds, b"example-app", &mut self.tables)
    }
    fn heard(&mut self, heard: Heard) -> Option<Step> {
        self.plan.heard(heard, self.bounds, b"example-app", &mut self.tables).unwrap()
    }
    fn head(&mut self, next: bool) {
        let mut head = Head::new();
        head.next = next;
        assert_eq!(self.heard(Heard::Head { status: 200, head }), None);
    }
    fn json(&mut self, kind: Kind, out: Writer, next: bool) -> Option<Step> {
        self.head(next);
        let (rows, result) = decode(kind, &bytes(out), self.chunk, &limits());
        for row in rows.into_boxed() {
            if let Some(step) = self.heard(Heard::Row(row)) {
                return Some(step);
            }
        }
        self.heard(Heard::Done(match result {
            Ok(doc) => Body::Document(doc),
            Err(err) => Body::Malformed(err),
        }))
    }
    fn statuses(&mut self, rows: &[document::Context], next: bool) -> Option<Step> {
        let mut out = Writer::new(4096);
        server::combined(&document::Combined { check: api::Check::Pending, total: 20 }, rows, &mut out).unwrap();
        self.json(Kind::CombinedStatus, out, next)
    }
    fn runs(&mut self, rows: &[document::Run], next: bool) -> Option<Step> {
        let mut out = Writer::new(4096);
        server::runs(rows, &mut out).unwrap();
        self.json(Kind::CheckRuns, out, next)
    }
    fn job(&mut self, row: &document::Job) -> Option<Step> {
        let mut out = Writer::new(4096);
        server::job(row, &mut out).unwrap();
        self.json(Kind::Job, out, false)
    }
}
fn started(bounds: Bounds, chunk: u32) -> Call {
    let mut call = Call::new(bounds, true, chunk);
    call.begin().unwrap();
    call
}
fn runs_started(bounds: Bounds) -> Call {
    let mut call = started(bounds, 7);
    call.statuses(&[], false);
    call
}
fn jobs_started() -> Call {
    let mut call = runs_started(bounds());
    assert_eq!(
        call.runs(
            &[run(11, api::Check::Failed, b"github-actions"), run(12, api::Check::Failed, b"github-actions")],
            false
        ),
        Some(Step::send(job_request(11)))
    );
    call
}
fn answer(step: Option<Step>) -> Box<[api::Status]> {
    let Some(Step::Done(Ok(api::Answer::Checks(rows)))) = step else {
        panic!("Checks answer: {step:?}");
    };
    rows
}
#[expect(clippy::unnecessary_wraps, reason = "expected verdicts retain the heard result shape")]
fn failed(error: api::Error) -> Option<Step> {
    Some(Step::Done(Err(error)))
}
#[test]
fn github_checks_pages_and_actions_fanout() {
    for chunk in [1, 7] {
        let mut call = Call::new(bounds(), false, chunk);
        assert_eq!(call.begin(), Ok(Step::send(Request::Repository { id: 7 })));
        call.head(false);
        let mut repo = fixtures::repository();
        repo.id = 7;
        repo.owner = Box::from(*b"acme");
        repo.name = Box::from(*b"widgets");
        assert_eq!(
            call.heard(Heard::Done(Body::Document(Document::Repository(repo)))),
            Some(Step::send(request(Kind::CombinedStatus, 1, 2)))
        );
        let context = fixtures::context(api::Check::Failed);
        assert_eq!(
            call.statuses(&[context.clone(), context], true),
            Some(Step::send(request(Kind::CombinedStatus, 2, 2)))
        );
        assert_eq!(call.statuses(&[], false), Some(Step::send(request(Kind::CheckRuns, 1, 2))));
        assert_eq!(
            call.runs(&[run(11, api::Check::Failed, b"github-actions")], true),
            Some(Step::send(request(Kind::CheckRuns, 2, 2)))
        );
        let mut second = run(12, api::Check::Failed, b"github-actions");
        second.completed = None;
        second.started = Some(fixtures::wall());
        assert_eq!(call.runs(&[second], false), Some(Step::send(job_request(11))));
        assert_eq!(call.job(&job(11)), Some(Step::send(job_request(12))));
        let rows = answer(call.job(&job(12)));
        assert_eq!(rows.len(), 4);
        for status in &rows[..2] {
            assert!(status.job.is_none());
        }
        for (status, id) in rows[2..].iter().zip([11, 12]) {
            assert_eq!(status.at, Time::from_nanos(fixtures::wall().as_nanos()));
            assert_eq!(status.context.as_ref(), b"build");
            assert_eq!(status.description.as_ref(), b"widgets");
            assert_eq!(status.url.as_ref(), b"https://example.invalid/check");
            assert_eq!(status.job, Some(api::JobAttempt { head: head(), run: job(id).run, job: id, attempt: 2 }));
        }
    }
    for (req, target) in [
        (
            request(Kind::CombinedStatus, 2, 2),
            "/repos/acme/widgets/commits/0123456789abcdef0123456789abcdef01234567/status?per_page=2&page=2",
        ),
        (
            request(Kind::CheckRuns, 3, 2),
            "/repos/acme/widgets/commits/0123456789abcdef0123456789abcdef01234567/check-runs?filter=latest&per_page=2&page=3",
        ),
        (job_request(11), "/repos/acme/widgets/actions/jobs/11"),
    ] {
        let encoded = request::encode(&req, &limits()).unwrap();
        assert_eq!(encoded.target.as_ref(), target.as_bytes());
        assert_eq!(encoded.accept, request::Accept::Json);
    }
}
#[test]
fn github_checks_other_apps_and_pull_regression() {
    for chunk in [1, 7] {
        let mut call = started(bounds(), chunk);
        call.statuses(&[], false);
        assert_eq!(
            call.runs(
                &[run(1, api::Check::Passed, b"github-actions"), run(2, api::Check::Pending, b"github-actions")],
                true
            ),
            Some(Step::send(request(Kind::CheckRuns, 2, 2)))
        );
        let mut other = run(3, api::Check::Failed, b"example-app");
        other.completed = None;
        other.started = None;
        other.name = Box::from([]);
        other.title = Box::from([]);
        other.url = Box::from([]);
        let rows = answer(call.runs(&[other], false));
        assert_eq!(rows.len(), 3);
        for status in &rows {
            assert!(status.job.is_none());
        }
        assert_eq!(rows[2].at, Time::ZERO);
        let (_, doc) = decode(Kind::CheckRuns, br#"{"check_runs":[{"status":"completed","conclusion":"neutral"},{"status":"completed","conclusion":"skipped"}]}"#, chunk, &limits());
        assert_eq!(doc, Ok(Document::Rows));
        let mut checks = runs_started(bounds());
        checks.head(false);
        assert_eq!(
            checks.heard(Heard::Row(Row::Run(document::Run::new(api::Check::Passed)))),
            failed(api::Error::InvalidAnswer)
        );
    }
}
#[test]
fn github_checks_bounds_and_page_cap() {
    let row = run(11, api::Check::Failed, b"github-actions");
    let width = u32::try_from(measure::STATUS_BYTES + row.name.len() + row.title.len() + row.url.len()).unwrap();
    for (bytes, expected) in [(width, true), (width - 1, false)] {
        let mut call = runs_started(Bounds { answer_bytes: bytes, ..bounds() });
        let result = call.runs(core::slice::from_ref(&row), false);
        if expected {
            assert_eq!(result, Some(Step::send(job_request(11))));
            assert_eq!(answer(call.job(&job(11))).len(), 1);
        } else {
            assert_eq!(result, failed(api::Error::TooLarge));
        }
    }
    let mut call = runs_started(Bounds { rows: 1, ..bounds() });
    assert_eq!(call.runs(core::slice::from_ref(&row), true), Some(Step::send(request(Kind::CheckRuns, 2, 1))));
    assert_eq!(call.runs(&[row], false), failed(api::Error::TooLarge));
    let mut call = runs_started(bounds());
    assert_eq!(
        call.runs(
            &[
                run(1, api::Check::Passed, b"example-app"),
                run(2, api::Check::Passed, b"example-app"),
                run(3, api::Check::Passed, b"example-app")
            ],
            false
        ),
        failed(api::Error::TooLarge)
    );
    for run_pages in [false, true] {
        let mut call = started(bounds(), 7);
        if run_pages {
            call.statuses(&[], false);
        }
        for page in 1..=50 {
            call.head(true);
            let document = if run_pages {
                Document::Rows
            } else {
                Document::Combined(document::Combined { check: api::Check::Pending, total: 0 })
            };
            let result = call.heard(Heard::Done(Body::Document(document)));
            if page == 50 {
                assert_eq!(result, failed(api::Error::TooLarge));
            } else {
                assert_eq!(
                    result,
                    Some(Step::send(request(
                        if run_pages { Kind::CheckRuns } else { Kind::CombinedStatus },
                        page + 1,
                        2
                    )))
                );
            }
        }
    }
    for invalid in
        [Bounds { rows: 0, ..bounds() }, Bounds { page_size: 0, ..bounds() }, Bounds { answer_bytes: 159, ..bounds() }]
    {
        assert_eq!(Call::new(invalid, true, 7).begin(), Err(api::Error::TooLarge));
    }
}
#[test]
fn github_checks_wrong_head_and_first_failure() {
    for wrong_id in [false, true] {
        let mut call = jobs_started();
        let mut row = job(11);
        if wrong_id {
            row.id = 99;
        } else {
            row.head = fixtures::pull(5).base_commit;
        }
        assert_eq!(call.job(&row), failed(api::Error::InvalidAnswer));
    }
    let mut call = runs_started(bounds());
    let mut row = run(11, api::Check::Passed, b"example-app");
    row.head = Some(fixtures::pull(5).base_commit);
    assert_eq!(call.runs(&[row], false), failed(api::Error::InvalidAnswer));
    for stage in 0_u8..3 {
        for status in [404, 410] {
            let mut call = match stage {
                0 => started(bounds(), 7),
                1 => runs_started(bounds()),
                _ => jobs_started(),
            };
            assert_eq!(call.heard(Heard::Head { status, head: Head::new() }), failed(api::Error::Missing));
            assert!(call.tables.repository(KEY).is_none());
            assert_eq!(
                call.plan.heard(Heard::Failed(Failure::Seen), bounds(), b"example-app", &mut call.tables),
                Err(api::Error::InvalidAnswer)
            );
        }
        for (failure, error) in [(Failure::Unseen, api::Error::Unavailable), (Failure::Seen, api::Error::Timeout)] {
            let mut call = match stage {
                0 => started(bounds(), 7),
                1 => runs_started(bounds()),
                _ => jobs_started(),
            };
            assert_eq!(call.heard(Heard::Failed(failure)), failed(error));
        }
        let mut call = match stage {
            0 => started(bounds(), 7),
            1 => runs_started(bounds()),
            _ => jobs_started(),
        };
        call.head(false);
        assert_eq!(
            call.heard(Heard::Done(Body::Document(Document::Branch(document::Branch {
                name: Box::from([]),
                commit: head()
            })))),
            failed(api::Error::InvalidAnswer)
        );
    }
}
#[test]
fn github_checks_metadata_round_trip() {
    for chunk in [1, 7] {
        let mut row = run(u64::MAX, api::Check::Failed, b"github-actions");
        row.started = Some(Wall::EPOCH);
        let mut out = Writer::new(4096);
        server::runs(core::slice::from_ref(&row), &mut out).unwrap();
        let (rows, doc) = decode(Kind::CheckRuns, &bytes(out), chunk, &limits());
        assert_eq!(doc, Ok(Document::Rows));
        assert_eq!(rows.as_slice(), &[Row::Run(row)]);
        let row = document::Job { id: u64::MAX, run: u64::MAX, attempt: u32::MAX, head: head() };
        let mut out = Writer::new(4096);
        server::job(&row, &mut out).unwrap();
        assert_eq!(decode(Kind::Job, &bytes(out), chunk, &limits()).1, Ok(Document::Job(row)));
    }
}
#[test]
fn github_checks_metadata_malformed() {
    let job = r#"{"id":11,"run_id":22,"run_attempt":2,"head_sha":"0123456789abcdef0123456789abcdef01234567","steps":[{"ignored":true}]}"#;
    for (json, expected) in [
        (job.replace("\"id\":11,", ""), Malformed::Missing { field: b"id" }),
        (job.replace("\"run_id\":22,", ""), Malformed::Missing { field: b"run_id" }),
        (job.replace("\"run_attempt\":2", "\"run_attempt\":4294967296"), Malformed::Range { field: b"run_attempt" }),
        (job.replace("\"id\":11", "\"id\":18446744073709551616"), Malformed::Range { field: b"id" }),
        (job.replace("\"id\":11", "\"id\":11,\"id\":11"), Malformed::Shape { field: b"id" }),
        (job.replace("0123456789abcdef0123456789abcdef01234567", "bad"), Malformed::Id { field: b"head_sha" }),
    ] {
        for chunk in [1, 7] {
            assert_eq!(decode(Kind::Job, json.as_bytes(), chunk, &limits()).1, Err(expected));
        }
    }
    for (field, value, expected) in [
        ("id", "\"bad\"", Malformed::Shape { field: b"id" }),
        ("head_sha", "\"BAD\"", Malformed::Id { field: b"head_sha" }),
        ("completed_at", "\"bad\"", Malformed::Time { field: b"completed_at" }),
        ("output", "[]", Malformed::Shape { field: b"" }),
        ("app", "\"bad\"", Malformed::Shape { field: b"" }),
    ] {
        let json =
            alloc::fmt::format(format_args!(r#"{{"check_runs":[{{"status":"in_progress","{field}":{value}}}]}}"#));
        for chunk in [1, 7] {
            assert_eq!(decode(Kind::CheckRuns, json.as_bytes(), chunk, &limits()).1, Err(expected));
        }
    }
    for length in [128, 129] {
        let json = alloc::fmt::format(format_args!(
            r#"{{"check_runs":[{{"status":"in_progress","name":"{}"}}]}}"#,
            "x".repeat(length)
        ));
        for chunk in [1, 7] {
            let result = decode(Kind::CheckRuns, json.as_bytes(), chunk, &limits()).1;
            if length == 128 {
                assert_eq!(result, Ok(Document::Rows));
            } else {
                assert_eq!(result, Err(Malformed::TooLong { field: b"name" }));
            }
        }
    }
}

#[test]
fn github_checks_metadata_defaults_and_bounds() {
    let minimal = br#"{"check_runs":[{"status":"in_progress","id":11,"head_sha":"0123456789abcdef0123456789abcdef01234567","completed_at":null,"started_at":null,"name":null,"output":null,"html_url":null,"app":null}]}"#;
    for chunk in [1, 7] {
        let mut call = started(bounds(), chunk);
        call.statuses(&[], false);
        let mut out = Writer::new(4096);
        out.put(minimal).unwrap();
        let rows = answer(call.json(Kind::CheckRuns, out, false));
        assert_eq!(
            rows[0],
            api::Status {
                at: Time::ZERO,
                context: Box::from([]),
                check: api::Check::Pending,
                description: Box::from([]),
                url: Box::from([]),
                job: None
            }
        );
        let (_, result) = decode(Kind::Job, br#"{"id":11,"run_id":22,"run_attempt":2,"head_sha":"0123456789abcdef0123456789abcdef01234567","steps":[{"name":"ignored","conclusion":"failure"}]}"#, chunk, &limits());
        assert_eq!(result, Ok(Document::Job(document::Job { id: 11, run: 22, attempt: 2, head: head() })));
    }
    for missing_id in [false, true] {
        let mut call = runs_started(bounds());
        let mut row = run(11, api::Check::Passed, b"example-app");
        if missing_id {
            row.id = None;
        } else {
            row.head = None;
        }
        assert_eq!(call.runs(&[row], false), failed(api::Error::InvalidAnswer));
    }
    let mut call = started(Bounds { rows: 1, ..bounds() }, 7);
    let context = fixtures::context(api::Check::Pending);
    assert_eq!(
        call.statuses(core::slice::from_ref(&context), true),
        Some(Step::send(request(Kind::CombinedStatus, 2, 1)))
    );
    assert_eq!(call.statuses(&[context], false), failed(api::Error::TooLarge));
    for stage in 0_u8..3 {
        for (malformed, error) in [
            (Malformed::Missing { field: b"id" }, api::Error::InvalidAnswer),
            (Malformed::Bound { field: b"name" }, api::Error::TooLarge),
        ] {
            let mut call = match stage {
                0 => started(bounds(), 7),
                1 => runs_started(bounds()),
                _ => jobs_started(),
            };
            call.head(false);
            assert_eq!(call.heard(Heard::Done(Body::Malformed(malformed))), failed(error));
        }
    }
    let mut row = run(11, api::Check::Passed, b"example-app");
    row.completed = Some(Wall::EPOCH);
    row.started = Some(fixtures::wall());
    let mut call = runs_started(bounds());
    assert_eq!(answer(call.runs(&[row], false))[0].at, Time::ZERO);
    let mut small = Writer::new(1);
    assert_eq!(server::job(&job(11), &mut small), Err(skein_lib::Overflow));
    assert_eq!(small.written(), 0);
}

#[test]
fn github_checks_display_bounds() {
    for chunk in [1, 7] {
        for field in ["name", "html_url", "output", "app"] {
            for length in [128, 129] {
                let text = "x".repeat(length);
                let value = if field == "output" {
                    alloc::fmt::format(format_args!(r#"{{"title":"{text}"}}"#))
                } else if field == "app" {
                    alloc::fmt::format(format_args!(r#"{{"slug":"{text}"}}"#))
                } else {
                    alloc::fmt::format(format_args!(r#""{text}""#))
                };
                let json = alloc::fmt::format(format_args!(
                    r#"{{"check_runs":[{{"status":"in_progress","{field}":{value}}}]}}"#
                ));
                let result = decode(Kind::CheckRuns, json.as_bytes(), chunk, &limits()).1;
                let key: &'static [u8] = match field {
                    "name" => b"name",
                    "html_url" => b"html_url",
                    "output" => b"title",
                    "app" => b"slug",
                    _ => unreachable!(),
                };
                if length == 128 {
                    assert_eq!(result, Ok(Document::Rows));
                } else {
                    assert_eq!(result, Err(Malformed::TooLong { field: key }));
                }
            }
        }
    }
}
