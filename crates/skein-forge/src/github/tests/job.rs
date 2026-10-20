use super::fixtures::{self, decode, limits};
use crate::{
    api,
    github::{
        decode::Malformed,
        document::{self, Document, Kind},
        header::{self, Head},
        plans::{
            Body, Failure, Heard,
            plan::{Bounds, Plan, Retention, Step},
            tables::{self, Tables},
        },
        redirect::{Origin, Redirect},
        request::{self, Auth, Method, Request},
    },
};

const KEY: api::Repository = api::Repository { forge: 3, repository: 7 };
const METADATA: &str =
    r#"{"id":11,"run_id":22,"run_attempt":2,"head_sha":"0123456789abcdef0123456789abcdef01234567","steps":STEPS}"#;
fn attempt() -> api::JobAttempt {
    api::JobAttempt { head: fixtures::pull(5).head_commit, run: 22, job: 11, attempt: 2 }
}
fn bounds() -> Bounds {
    Bounds { inner: 4, page_size: 2, rows: 4, answer_bytes: 4096, body_bytes: 4096 }
}
fn metadata() -> document::Job {
    let attempt = attempt();
    document::Job { id: attempt.job, run: attempt.run, attempt: attempt.attempt, head: attempt.head, step: None }
}
fn request(log: bool) -> Request {
    let owner = Box::from(*b"acme");
    let name = Box::from(*b"widgets");
    if log { Request::JobLog { owner, name, id: 11 } } else { Request::Job { owner, name, id: 11 } }
}
fn failed(error: api::Error) -> Step {
    Step::Done(Err(error))
}
struct Call {
    plan: Plan,
    tables: Tables,
    bounds: Bounds,
}
impl Call {
    fn new(max_bytes: u32, cached: bool, bounds: Bounds) -> Self {
        let mut tables = Tables::new(2, 2);
        if cached {
            tables.keep_repository(
                KEY,
                tables::Repository {
                    owner: Box::from(*b"acme"),
                    name: Box::from(*b"widgets"),
                    owner_id: 8,
                    merge: Some(tables::Method::Merge),
                    html_url: Box::from(*b"https://example.test/acme/widgets"),
                    organization: true,
                },
            );
        }
        let mut plan = Plan::new(KEY, api::Op::Read(api::Read::Job { attempt: attempt(), max_bytes }));
        assert_eq!(
            plan.begin(bounds, b"example-app", &mut tables),
            Ok(Step::send(if cached { request(false) } else { Request::Repository { id: 7 } }))
        );
        Self { plan, tables, bounds }
    }
    fn heard(&mut self, heard: Heard) -> Option<Step> {
        self.plan.heard(heard, self.bounds, b"example-app", &mut self.tables).unwrap()
    }
    fn head(&mut self, status: u16, location: Option<&[u8]>) -> Option<Step> {
        let mut head = Head::new();
        if let Some(location) = location {
            header::field(&mut head, b"Location", location, &limits());
        }
        self.heard(Heard::Head { status, head })
    }
    fn job(&mut self, job: document::Job) -> Option<Step> {
        assert_eq!(self.head(200, None), None);
        self.heard(Heard::Done(Body::Document(Document::Job(job))))
    }
    fn logs(&mut self) {
        assert_eq!(self.job(metadata()), Some(Step::send(request(true))));
    }
    fn redirected(&mut self) {
        self.logs();
        let Some(Step::Redirect { .. }) = self.head(302, Some(b"https://logs.example.test/job.txt?token=invented"))
        else {
            panic!("redirect")
        };
    }
}
#[test]
fn job_metadata_selects_the_first_failure_and_no_step_when_none_failed() {
    for (steps, expected) in [
        (
            r#"[{"number":1,"name":"prepare","conclusion":"success"},{"conclusion":"failure","name":"build","number":2},{"number":3,"name":"later","conclusion":"failure"}]"#,
            Some(api::Step { number: 2, name: Box::from(*b"build") }),
        ),
        (
            r#"[{"number":1,"name":"prepare","conclusion":null},{"number":2,"name":"build","conclusion":"success"}]"#,
            None,
        ),
        ("[]", None),
    ] {
        for chunk in [1, 7, 4096] {
            let input = METADATA.replace("STEPS", steps);
            let (rows, decoded) = decode(Kind::Job, input.as_bytes(), chunk, &limits());
            assert!(rows.is_empty());
            let Document::Job(job) = decoded.unwrap() else { panic!("metadata") };
            assert_eq!(job, document::Job { step: expected.clone(), ..metadata() });
            let mut call = Call::new(8, true, bounds());
            assert_eq!(call.job(job), Some(Step::send(request(true))));
            assert!(call.head(302, Some(b"https://logs.example.test/job")).is_some());
            assert_eq!(call.head(200, None), None);
            assert_eq!(
                call.heard(Heard::Done(Body::Raw { bytes: Box::from(*b"log"), more: false })),
                Some(Step::Done(Ok(api::Answer::Job {
                    attempt: attempt(),
                    step: expected.clone(),
                    log: Box::from(*b"log"),
                    truncated: false,
                })))
            );
        }
    }
}
#[test]
fn job_metadata_bounds_and_malformed_steps_reject_the_answer() {
    for (steps, error) in [
        (r#"[{"name":"build","conclusion":"failure"}]"#, Malformed::Missing { field: b"number" }),
        (r#"[{"number":4294967296,"name":"build","conclusion":"failure"}]"#, Malformed::Range { field: b"number" }),
    ] {
        assert_eq!(decode(Kind::Job, METADATA.replace("STEPS", steps).as_bytes(), 1, &limits()).1, Err(error));
    }
    let mut limits = limits();
    limits.inner = 1;
    let input = METADATA.replace(
        "STEPS",
        r#"[{"number":1,"name":"build","conclusion":"failure"},{"number":2,"name":"later","conclusion":"success"}]"#,
    );
    let error = decode(Kind::Job, input.as_bytes(), 7, &limits).1.unwrap_err();
    let mut call = Call::new(8, true, bounds());
    call.head(200, None);
    assert_eq!(call.heard(Heard::Done(Body::Malformed(error))), Some(failed(api::Error::TooLarge)));
}
#[test]
fn job_missing_statuses_and_each_attempt_mismatch_are_missing_job() {
    for status in [404, 410] {
        for logs in [false, true] {
            let mut call = Call::new(8, true, bounds());
            if logs {
                call.logs();
            }
            assert_eq!(call.head(status, None), Some(failed(api::Error::MissingJob)));
            assert!(call.tables.repository(KEY).is_none());
        }
    }
    for wrong in [
        document::Job { run: 23, ..metadata() },
        document::Job { attempt: 3, ..metadata() },
        document::Job { head: fixtures::pull(5).base_commit, ..metadata() },
        document::Job { id: 12, ..metadata() },
    ] {
        let mut call = Call::new(8, true, bounds());
        assert_eq!(call.job(wrong), Some(failed(api::Error::MissingJob)));
    }
}
#[test]
fn job_logs_yield_http_and_https_redirects_with_tail_retention() {
    for (location, tls, port, target) in [
        (
            b"https://logs.example.test/job.txt?token=invented".as_slice(),
            true,
            443,
            b"/job.txt?token=invented".as_slice(),
        ),
        (b"http://logs.example.test:8080/job.txt", false, 8080, b"/job.txt"),
        (b"http://logs.example.test?part=1", false, 80, b"/?part=1"),
    ] {
        let mut call = Call::new(8, true, bounds());
        call.logs();
        assert_eq!(
            call.head(302, Some(location)),
            Some(Step::Redirect {
                redirect: Redirect {
                    origin: Origin { tls, host: Box::from(*b"logs.example.test"), port },
                    target: Box::from(target)
                },
                retention: Retention::Tail { bytes: 8 },
            })
        );
    }
    for (request, target) in [
        (request(false), b"/repos/acme/widgets/actions/jobs/11".as_slice()),
        (request(true), b"/repos/acme/widgets/actions/jobs/11/logs"),
    ] {
        let encoded = request::encode(&request, &limits()).unwrap();
        assert_eq!(encoded.method, Method::Get);
        assert_eq!(encoded.target.as_ref(), target);
        assert_eq!(encoded.body, None);
        assert_eq!(encoded.auth, Auth::Installation);
    }
}
#[test]
fn job_log_requires_a_302_with_an_absolute_http_location() {
    for (status, location, error) in [
        (302, None, api::Error::InvalidAnswer),
        (302, Some(b"/job.txt".as_slice()), api::Error::InvalidAnswer),
        (302, Some(b"ftp://logs.example.test/job.txt".as_slice()), api::Error::InvalidAnswer),
        (200, None, api::Error::InvalidAnswer),
        (307, Some(b"https://logs.example.test/job.txt".as_slice()), api::Error::Unavailable),
        (500, None, api::Error::Timeout),
    ] {
        let mut call = Call::new(8, true, bounds());
        call.logs();
        assert_eq!(call.head(status, location), Some(failed(error)));
    }
}
#[test]
fn job_log_keeps_the_tail_and_reports_bytes_read_before_it() {
    for (input, more, output, truncated) in [
        (b"0123456789".as_slice(), false, b"23456789".as_slice(), true),
        (b"short", false, b"short", false),
        (b"23456789", true, b"23456789", true),
        (b"12345678", false, b"12345678", false),
        (b"", false, b"", false),
    ] {
        let mut call = Call::new(8, true, bounds());
        call.redirected();
        assert_eq!(call.head(200, None), None);
        assert_eq!(
            call.heard(Heard::Done(Body::Raw { bytes: Box::from(input), more })),
            Some(Step::Done(Ok(api::Answer::Job {
                attempt: attempt(),
                step: None,
                log: Box::from(output),
                truncated,
            })))
        );
        assert_eq!(
            call.plan.heard(Heard::Failed(Failure::Seen), bounds(), b"example-app", &mut call.tables),
            Err(api::Error::InvalidAnswer)
        );
    }
    let mut call = Call::new(0, true, bounds());
    call.redirected();
    call.head(200, None);
    assert_eq!(
        call.heard(Heard::Done(Body::Raw { bytes: Box::from(*b"log"), more: false })),
        Some(Step::Done(Ok(api::Answer::Job { attempt: attempt(), step: None, log: Box::from([]), truncated: true })))
    );
}
#[test]
fn job_followed_redirect_cannot_redirect_again() {
    for status in [301, 302, 303, 304, 307, 308] {
        let mut call = Call::new(8, true, bounds());
        call.redirected();
        assert_eq!(call.head(status, Some(b"https://logs.example.test/again")), Some(failed(api::Error::Unavailable)));
    }
}
#[test]
fn job_resolves_the_repository_and_fits_log_beside_the_failed_step() {
    let mut call = Call::new(8, false, bounds());
    call.head(200, None);
    let mut repo = fixtures::repository();
    repo.id = 7;
    repo.owner = Box::from(*b"acme");
    repo.name = Box::from(*b"widgets");
    assert_eq!(call.heard(Heard::Done(Body::Document(Document::Repository(repo)))), Some(Step::send(request(false))));
    call.bounds.answer_bytes = 6;
    assert_eq!(
        call.job(document::Job { step: Some(api::Step { number: 2, name: Box::from(*b"build") }), ..metadata() }),
        Some(Step::send(request(true)))
    );
    let Some(Step::Redirect { retention, .. }) = call.head(302, Some(b"https://logs.example.test/job")) else {
        panic!("redirect")
    };
    assert_eq!(retention, Retention::Tail { bytes: 1 });
    call.head(200, None);
    assert_eq!(
        call.heard(Heard::Done(Body::Raw { bytes: Box::from(*b"log"), more: false })),
        Some(Step::Done(Ok(api::Answer::Job {
            attempt: attempt(),
            step: Some(api::Step { number: 2, name: Box::from(*b"build") }),
            log: Box::from(*b"g"),
            truncated: true,
        })))
    );
}

#[test]
fn followed_request_preserves_target_and_refuses_one_past_the_limit() {
    let limits = limits();
    let cap = usize::try_from(limits.target_bytes).unwrap();
    for target in [b"/job%2Fpart?signature=a%2Bb&part=1".as_slice(), &fixtures::filled(cap, b'x')] {
        let redirect = Redirect {
            origin: Origin { tls: true, host: Box::from(*b"logs.example.test"), port: 443 },
            target: Box::from(target),
        };
        let followed = crate::github::redirect::request(&redirect);
        assert_eq!(followed, Request::Followed { target: Box::from(target) });
        let encoded = request::encode(&followed, &limits).unwrap();
        assert_eq!(encoded.target.as_ref(), target);
        assert_eq!(encoded.method, Method::Get);
        assert_eq!(encoded.auth, Auth::None);
        assert_eq!(encoded.body, None);
        assert_eq!(request::fields(&encoded), request::fields(&request::encode(&request(true), &limits).unwrap()));
        assert_eq!(crate::github::decode::kind(&followed), None);
    }
    let request = Request::Followed { target: fixtures::filled(cap + 1, b'x') };
    assert_eq!(request::encode(&request, &limits), Err(request::Refusal::TooLong));
}
