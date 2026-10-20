//! Contents plans through HTTP bytes, with literal answers and exchange counts.
use super::fixtures;
use alloc::boxed::Box;
use skein_forge::api::{Answer, Error, File, Op, Read};
use skein_lib::Duration;
const HEAD: [u8; 32] = {
    let mut bytes = [0; 32];
    bytes[0] = 1;
    bytes
};
const BASE: [u8; 32] = {
    let mut bytes = [0; 32];
    bytes[0] = 3;
    bytes
};
const PULL: &[u8] = br#"{"number":1,"state":"open","head":{"repo_id":1,"ref":"work","sha":"0100000000000000000000000000000000000000"},"base":{"repo_id":1,"ref":"main","sha":"0300000000000000000000000000000000000000"},"merged":false,"merge_commit_sha":null,"merge_base":"0300000000000000000000000000000000000000","mergeable":true,"requested_reviewers":[]}"#;
#[test]
fn pull_files_at_another_head_fail_stale_reading_no_file() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::PullFiles { number: 1, head: [0; 32], page: 1 }));
    h.run(Duration::from_millis(1));
    let lines = h.request_lines(fixtures::FORGE).into_boxed();
    assert_eq!(lines.last().expect("pull request").as_ref(), b"GET /api/v1/repos/fixture/specimen/pulls/1 HTTP/1.1");
    h.answer(fixtures::FORGE, &fixtures::ok(PULL));
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("one answer");
    assert_eq!((answer.cost, answer.result.clone()), (1, Err(Error::Stale)));
}
#[test]
fn a_files_missing_side_is_absent_and_its_read_counted() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::PullFiles { number: 1, head: HEAD, page: 1 }));
    h.answer(fixtures::FORGE, &fixtures::ok(PULL));
    h.answer(fixtures::FORGE, b"HTTP/1.1 200 OK\r\nContent-Length: 40\r\nX-HasMore: true\r\n\r\n[{\"filename\":\"one\",\"status\":\"modified\"}]");
    h.answer(fixtures::FORGE, &fixtures::status(404));
    h.answer(fixtures::FORGE, &fixtures::ok(b"after"));
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("one answer");
    assert_eq!(
        (answer.cost, answer.result.clone()),
        (
            4,
            Ok(Answer::PullFiles {
                head: HEAD,
                files: Box::new([File {
                    path: Box::from(&b"one"[..]),
                    before: None,
                    after: Some(Box::from(&b"after"[..]))
                }]),
                more: true
            })
        )
    );
    let lines = h.request_lines(fixtures::FORGE).into_boxed();
    assert_eq!(
        lines[4].as_ref(),
        b"GET /api/v1/repos/fixture/specimen/raw/one?ref=0300000000000000000000000000000000000000 HTTP/1.1"
    );
    assert_eq!(
        lines[5].as_ref(),
        b"GET /api/v1/repos/fixture/specimen/raw/one?ref=0100000000000000000000000000000000000000 HTTP/1.1"
    );
}
#[test]
fn pull_files_choose_added_deleted_and_renamed_sides() {
    for (name, document, first, file, cost) in side_cases() {
        let mut h = fixtures::known();
        h.call(1, fixtures::REPO, Op::Read(Read::PullFiles { number: 1, head: HEAD, page: 2 }));
        h.answer(fixtures::FORGE, &fixtures::ok(PULL));
        h.answer(fixtures::FORGE, &fixtures::ok(document));
        assert!(skein_lib::bytes::find(&h.request_bytes(fixtures::FORGE), first).is_some(), "{name}: first side");
        h.answer(fixtures::FORGE, &fixtures::ok(b"text"));
        if name == "renamed" {
            h.answer(fixtures::FORGE, &fixtures::ok(b"text"));
        }
        h.run(Duration::from_millis(10));
        let answer = h.answered(1).expect("one answer");
        assert_eq!(
            (answer.cost, answer.result.clone()),
            (cost, Ok(Answer::PullFiles { head: HEAD, files: Box::new([file]), more: false })),
            "{name}"
        );
    }
}
type SideCase = (&'static str, &'static [u8], &'static [u8], File, u32);
fn side_cases() -> [SideCase; 3] {
    [
        (
            "added",
            br#"[{"filename":"new","status":"added"}]"#,
            b"raw/new?ref=0100000000000000000000000000000000000000",
            File { path: Box::from(&b"new"[..]), before: None, after: Some(Box::from(&b"text"[..])) },
            3,
        ),
        (
            "deleted",
            br#"[{"filename":"old","status":"deleted"}]"#,
            b"raw/old?ref=0300000000000000000000000000000000000000",
            File { path: Box::from(&b"old"[..]), before: Some(Box::from(&b"text"[..])), after: None },
            3,
        ),
        (
            "renamed",
            br#"[{"filename":"new","previous_filename":"old","status":"renamed"}]"#,
            b"raw/old?ref=0300000000000000000000000000000000000000",
            File {
                path: Box::from(&b"new"[..]),
                before: Some(Box::from(&b"text"[..])),
                after: Some(Box::from(&b"text"[..])),
            },
            4,
        ),
    ]
}
#[test]
fn comparisons_read_both_directions_and_both_file_sides() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::Compare { before: BASE, after: HEAD }));
    h.answer(fixtures::FORGE, &fixtures::ok(br#"{"total_commits":1,"commits":[{"sha":"0100000000000000000000000000000000000000"}],"files":[{"filename":"file","status":"modified"}]}"#));
    h.answer(fixtures::FORGE, &fixtures::ok(br#"{"total_commits":0,"commits":[],"files":[]}"#));
    h.answer(fixtures::FORGE, &fixtures::ok(b"before"));
    h.answer(fixtures::FORGE, &fixtures::ok(b"after"));
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("comparison");
    assert_eq!(
        (answer.cost, answer.result.clone()),
        (
            4,
            Ok(Answer::Compare {
                before: BASE,
                after: HEAD,
                contains_before: true,
                files: Box::new([File {
                    path: Box::from(&b"file"[..]),
                    before: Some(Box::from(&b"before"[..])),
                    after: Some(Box::from(&b"after"[..]))
                }]),
                commits: Box::new([HEAD])
            })
        )
    );
    let lines = h.request_lines(fixtures::FORGE).into_boxed();
    assert_eq!(lines[2].as_ref(), b"GET /api/v1/repos/fixture/specimen/compare/0300000000000000000000000000000000000000...0100000000000000000000000000000000000000?verification=false HTTP/1.1");
    assert_eq!(lines[3].as_ref(), b"GET /api/v1/repos/fixture/specimen/compare/0100000000000000000000000000000000000000...0300000000000000000000000000000000000000?files=false&verification=false HTTP/1.1");
}
#[test]
fn file_reads_send_the_range_and_keep_only_the_requested_bytes() {
    for (name, body, max, range, expected, truncated) in [
        ("short", &b"abc"[..], 4, &b"Range: bytes=0-4\r\n"[..], &b"abc"[..], false),
        ("full", &b"abcd"[..], 4, &b"Range: bytes=0-4\r\n"[..], &b"abcd"[..], false),
        ("long", &b"abcde"[..], 4, &b"Range: bytes=0-4\r\n"[..], &b"abcd"[..], true),
        ("empty", &b"x"[..], 0, &b"Range: bytes=0-0\r\n"[..], &b""[..], true),
    ] {
        let mut h = fixtures::known();
        h.call(1, fixtures::REPO, Op::Read(Read::File { head: HEAD, path: Box::from(&b"a b/c"[..]), max_bytes: max }));
        h.run(Duration::from_millis(1));
        let request = h.request_bytes(fixtures::FORGE);
        assert!(skein_lib::bytes::find(&request, range).is_some(), "{name}: range");
        assert!(skein_lib::bytes::find(&request, b"raw/a%20b/c?").is_some(), "{name}: path");
        h.answer(fixtures::FORGE, &fixtures::ok(body));
        h.run(Duration::from_millis(10));
        let answer = h.answered(1).expect("file");
        assert_eq!(
            (answer.cost, answer.result.clone()),
            (1, Ok(Answer::File { head: HEAD, path: Box::from(&b"a b/c"[..]), bytes: Box::from(expected), truncated })),
            "{name}"
        );
    }
}
#[test]
fn raw_sides_at_the_answer_bound_fit_and_one_byte_more_is_too_large() {
    for (name, body, expected) in bounded_cases() {
        let mut shared = fixtures::shared();
        shared.answer_bytes = 4096;
        let mut h = fixtures::Harness::with(fixtures::limits(), shared, fixtures::settings());
        h.connected(fixtures::FORGE);
        h.answer(fixtures::FORGE, &fixtures::user_ok());
        h.answer(fixtures::FORGE, &fixtures::settings_ok());
        h.known(fixtures::REPO, fixtures::specimen());
        h.call(1, fixtures::REPO, Op::Read(Read::PullFiles { number: 1, head: HEAD, page: 1 }));
        h.answer(fixtures::FORGE, &fixtures::ok(PULL));
        h.answer(fixtures::FORGE, &fixtures::last_page(br#"[{"filename":"x","status":"added"}]"#));
        h.answer(fixtures::FORGE, &fixtures::ok(body));
        h.run(Duration::from_millis(10));
        let answer = h.answered(1).expect("bounded answer");
        assert_eq!(answer.result, expected, "{name}");
    }
}
type BoundedCase = (&'static str, &'static [u8], Result<Answer, Error>);
fn bounded_cases() -> [BoundedCase; 2] {
    [
        (
            "exact",
            &[b'x'; 4031],
            Ok(Answer::PullFiles {
                head: HEAD,
                files: Box::new([File {
                    path: Box::from(&b"x"[..]),
                    before: None,
                    after: Some(Box::from([b'x'; 4031])),
                }]),
                more: false,
            }),
        ),
        ("past", &[b'x'; 4032], Err(Error::TooLarge)),
    ]
}
#[test]
fn multiple_changed_files_each_advance_to_their_own_sides() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::PullFiles { number: 1, head: HEAD, page: 1 }));
    h.answer(fixtures::FORGE, &fixtures::ok(PULL));
    h.answer(
        fixtures::FORGE,
        &fixtures::last_page(br#"[{"filename":"new","status":"added"},{"filename":"old","status":"deleted"}]"#),
    );
    h.answer(fixtures::FORGE, &fixtures::ok(b"new text"));
    h.answer(fixtures::FORGE, &fixtures::ok(b"old text"));
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("files");
    let expected = Answer::PullFiles {
        head: HEAD,
        files: Box::new([
            File { path: Box::from(&b"new"[..]), before: None, after: Some(Box::from(&b"new text"[..])) },
            File { path: Box::from(&b"old"[..]), before: Some(Box::from(&b"old text"[..])), after: None },
        ]),
        more: false,
    };
    assert_eq!((answer.cost, answer.result.clone()), (4, Ok(expected)));
}
#[test]
fn file_reads_accept_partial_content_and_preserve_raw_bytes() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::File { head: HEAD, path: Box::from(&b"file"[..]), max_bytes: 2 }));
    h.answer(
        fixtures::FORGE,
        b"HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: bytes 0-2/9\r\n\r\n\xff\x00x",
    );
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("file");
    assert_eq!(
        (answer.cost, answer.result.clone()),
        (
            1,
            Ok(Answer::File {
                head: HEAD,
                path: Box::from(&b"file"[..]),
                bytes: Box::from(&b"\xff\x00"[..]),
                truncated: true
            })
        )
    );
}
#[test]
fn contents_sizes_agree_with_the_clients_measure() {
    for (name, answer, expected) in measured_contents() {
        assert_eq!(
            skein_forge::forgejo::plans::reads::size::answer(&answer, 8),
            Ok(expected),
            "{name}: running measure"
        );
        assert_eq!(skein_forge::measure::answer(&answer), Some(expected), "{name}: client measure");
    }
}
fn measured_contents() -> [(&'static str, Answer, u64); 4] {
    [
        (
            "job with its step",
            Answer::Job {
                attempt: ATTEMPT,
                step: Some(Step { number: 1, name: Box::from(&b"build"[..]) }),
                log: Box::from(&b"abc"[..]),
                truncated: false,
            },
            8,
        ),
        (
            "file",
            Answer::File { head: HEAD, path: Box::from(&b"x"[..]), bytes: Box::from(&b"abc"[..]), truncated: false },
            4,
        ),
        (
            "pull files",
            Answer::PullFiles {
                head: HEAD,
                files: Box::new([File {
                    path: Box::from(&b"x"[..]),
                    before: Some(Box::from(&b"a"[..])),
                    after: Some(Box::from(&b"bc"[..])),
                }]),
                more: false,
            },
            68,
        ),
        (
            "comparison",
            Answer::Compare {
                before: BASE,
                after: HEAD,
                contains_before: true,
                files: Box::new([File {
                    path: Box::from(&b"x"[..]),
                    before: Some(Box::from(&b"a"[..])),
                    after: Some(Box::from(&b"bc"[..])),
                }]),
                commits: Box::new([HEAD]),
            },
            100,
        ),
    ]
}
use skein_forge::api::{Check, JobAttempt, Status, Step};
const ATTEMPT: JobAttempt = JobAttempt { head: HEAD, run: 7, job: 10, attempt: 3 };
const RUN: &[u8] = br#"{"id":7,"index_in_repo":5,"commit_sha":"0100000000000000000000000000000000000000"}"#;
const RUNS: &[u8] = br#"{"total_count":1,"workflow_runs":[{"id":7,"index_in_repo":5,"commit_sha":"0100000000000000000000000000000000000000"}]}"#;
const JOB: &[u8] = br#"{"id":10,"run_id":7,"attempt":3,"steps":[{"number":0,"name":"Set up job","status":"success"},{"number":1,"name":"build","status":"failure"},{"number":2,"name":"Complete job","status":"failure"}]}"#;
const JOBS: &[u8] = br#"[{"id":9,"run_id":7,"attempt":1},{"id":10,"run_id":7,"attempt":3}]"#;
const FAILING: &[u8] = br#"{"state":"failure","total_count":1,"statuses":[{"creator":null,"updated_at":"1970-01-01T00:00:00Z","context":"build","status":"failure","description":"failed","target_url":"/fixture/specimen/actions/runs/5/jobs/1"}]}"#;
fn failing(job: Option<JobAttempt>) -> Status {
    Status {
        at: skein_lib::Time::ZERO,
        context: Box::from(&b"build"[..]),
        check: Check::Failed,
        description: Box::from(&b"failed"[..]),
        url: Box::from(&b"http://127.0.0.1:3000/fixture/specimen/actions/runs/5/jobs/1"[..]),
        job,
    }
}
#[test]
fn checks_find_a_failing_jobs_run_and_attempt_from_its_link() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::Checks { commit: HEAD }));
    h.answer(fixtures::FORGE, &fixtures::ok(FAILING));
    h.answer(fixtures::FORGE, &fixtures::ok(RUNS));
    h.answer(fixtures::FORGE, &fixtures::ok(JOBS));
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("checks");
    assert_eq!((answer.cost, answer.result.clone()), (3, Ok(Answer::Checks(Box::new([failing(Some(ATTEMPT))])))));
    let lines = h.request_lines(fixtures::FORGE).into_boxed();
    assert_eq!(lines[3].as_ref(), b"GET /api/v1/repos/fixture/specimen/actions/runs?head_sha=0100000000000000000000000000000000000000&run_number=5&page=1&limit=1 HTTP/1.1");
    assert_eq!(lines[4].as_ref(), b"GET /api/v1/repos/fixture/specimen/actions/runs/7/jobs HTTP/1.1");
}
#[test]
fn checks_leave_the_job_absent_when_the_run_or_index_is_missing() {
    for (name, runs, jobs, cost) in [
        ("no run", &br#"{"total_count":0,"workflow_runs":[]}"#[..], None, 2),
        ("no index", RUNS, Some(&br#"[{"id":9,"run_id":7,"attempt":1}]"#[..]), 3),
        ("other run", RUNS, Some(&br#"[{"id":9,"run_id":7,"attempt":1},{"id":10,"run_id":8,"attempt":3}]"#[..]), 3),
    ] {
        let mut h = fixtures::known();
        h.call(1, fixtures::REPO, Op::Read(Read::Checks { commit: HEAD }));
        h.answer(fixtures::FORGE, &fixtures::ok(FAILING));
        h.answer(fixtures::FORGE, &fixtures::ok(runs));
        if let Some(jobs) = jobs {
            h.answer(fixtures::FORGE, &fixtures::ok(jobs));
        }
        h.run(Duration::from_millis(10));
        let answer = h.answered(1).expect("checks");
        assert_eq!(
            (answer.cost, answer.result.clone()),
            (cost, Ok(Answer::Checks(Box::new([failing(None)])))),
            "{name}"
        );
    }
}
#[test]
fn checks_leave_the_job_absent_when_an_actions_lookup_answers_404() {
    for (name, runs, cost) in [("run lookup", None, 2), ("jobs lookup", Some(RUNS), 3)] {
        let mut h = fixtures::known();
        h.call(1, fixtures::REPO, Op::Read(Read::Checks { commit: HEAD }));
        h.answer(fixtures::FORGE, &fixtures::ok(FAILING));
        if let Some(runs) = runs {
            h.answer(fixtures::FORGE, &fixtures::ok(runs));
        }
        h.answer(fixtures::FORGE, &fixtures::status(404));
        h.run(Duration::from_millis(10));
        let answer = h.answered(1).expect("checks");
        assert_eq!(
            (answer.cost, answer.result.clone()),
            (cost, Ok(Answer::Checks(Box::new([failing(None)])))),
            "{name}"
        );
    }
}
#[test]
fn checks_read_every_status_page_until_one_comes_short() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::Checks { commit: HEAD }));
    h.answer(fixtures::FORGE, &fixtures::ok(br#"{"state":"success","total_count":2,"statuses":[{"creator":null,"updated_at":"1970-01-01T00:00:00Z","context":"a","status":"success","description":"","target_url":""},{"creator":null,"updated_at":"1970-01-01T00:00:00Z","context":"b","status":"pending","description":"","target_url":""}]}"#));
    h.answer(fixtures::FORGE, &fixtures::ok(br#"{"state":"","total_count":0,"statuses":[]}"#));
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("checks");
    let expected = Box::new([
        Status {
            at: skein_lib::Time::ZERO,
            context: Box::from(&b"a"[..]),
            check: Check::Passed,
            description: Box::from([]),
            url: Box::from([]),
            job: None,
        },
        Status {
            at: skein_lib::Time::ZERO,
            context: Box::from(&b"b"[..]),
            check: Check::Pending,
            description: Box::from([]),
            url: Box::from([]),
            job: None,
        },
    ]);
    assert_eq!((answer.cost, answer.result.clone()), (2, Ok(Answer::Checks(expected))));
    let lines = h.request_lines(fixtures::FORGE).into_boxed();
    assert_eq!(lines[3].as_ref(), b"GET /api/v1/repos/fixture/specimen/commits/0100000000000000000000000000000000000000/status?page=2&limit=2 HTTP/1.1");
}
#[test]
fn a_job_log_at_another_head_is_missing_job() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::Job { attempt: ATTEMPT, max_bytes: 4 }));
    h.run(Duration::from_millis(1));
    assert_eq!(
        h.request_lines(fixtures::FORGE).into_boxed().last().expect("run").as_ref(),
        b"GET /api/v1/repos/fixture/specimen/actions/runs/7 HTTP/1.1"
    );
    h.answer(
        fixtures::FORGE,
        &fixtures::ok(br#"{"id":7,"index_in_repo":5,"commit_sha":"0300000000000000000000000000000000000000"}"#),
    );
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("missing job");
    assert_eq!((answer.cost, answer.result.clone()), (1, Err(Error::MissingJob)));
}
#[test]
fn a_job_of_another_id_run_or_attempt_is_missing_job() {
    for (name, jobs) in [
        ("other id", &br#"{"id":9,"run_id":7,"attempt":3}"#[..]),
        ("other attempt", &br#"{"id":10,"run_id":7,"attempt":2}"#[..]),
        ("other run", &br#"{"id":10,"run_id":8,"attempt":3}"#[..]),
    ] {
        let mut h = fixtures::known();
        h.call(1, fixtures::REPO, Op::Read(Read::Job { attempt: ATTEMPT, max_bytes: 4 }));
        h.answer(fixtures::FORGE, &fixtures::ok(RUN));
        h.answer(fixtures::FORGE, &fixtures::ok(jobs));
        h.run(Duration::from_millis(10));
        let answer = h.answered(1).expect("missing job");
        assert_eq!((answer.cost, answer.result.clone()), (2, Err(Error::MissingJob)), "{name}");
    }
}
#[test]
fn a_job_read_answered_404_is_missing_job_at_each_exchange() {
    for (name, before, cost) in [("run", &[][..], 1), ("job", &[RUN][..], 2), ("log", &[RUN, JOB][..], 3)] {
        let mut h = fixtures::known();
        h.call(1, fixtures::REPO, Op::Read(Read::Job { attempt: ATTEMPT, max_bytes: 4 }));
        for document in before {
            h.answer(fixtures::FORGE, &fixtures::ok(document));
        }
        h.answer(fixtures::FORGE, &fixtures::status(404));
        h.run(Duration::from_millis(10));
        let answer = h.answered(1).expect("missing job");
        assert_eq!((answer.cost, answer.result.clone()), (cost, Err(Error::MissingJob)), "{name}");
    }
}
#[test]
fn a_job_log_asks_a_suffix_range_and_bytes_before_it_are_truncated() {
    for (name, response, log, truncated) in [
        (
            "short",
            &b"HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: bytes 0-2/3\r\n\r\nabc"[..],
            &b"abc"[..],
            false,
        ),
        ("past max", &b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nabcdef"[..], &b"cdef"[..], true),
        (
            "range names more",
            &b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 16-19/20\r\n\r\nabcd"[..],
            &b"abcd"[..],
            true,
        ),
    ] {
        let mut h = fixtures::known();
        h.call(1, fixtures::REPO, Op::Read(Read::Job { attempt: ATTEMPT, max_bytes: 4 }));
        h.answer(fixtures::FORGE, &fixtures::ok(RUN));
        h.answer(fixtures::FORGE, &fixtures::ok(JOB));
        let request = h.request_bytes(fixtures::FORGE);
        assert!(
            skein_lib::bytes::find(
                &request,
                b"GET /api/v1/repos/fixture/specimen/actions/jobs/10/logs?attempt=3&step=1 HTTP/1.1\r\n"
            )
            .is_some(),
            "{name}: log"
        );
        assert!(skein_lib::bytes::find(&request, b"Range: bytes=-4\r\n").is_some(), "{name}: range");
        h.answer(fixtures::FORGE, response);
        h.run(Duration::from_millis(10));
        let answer = h.answered(1).expect("log");
        assert_eq!(
            (answer.cost, answer.result.clone()),
            (
                3,
                Ok(Answer::Job {
                    attempt: ATTEMPT,
                    step: Some(Step { number: 1, name: Box::from(&b"build"[..]) }),
                    log: Box::from(log),
                    truncated
                })
            ),
            "{name}"
        );
    }
}

fn job(statuses: [&str; 3]) -> Box<[u8]> {
    let mut out = skein_lib::List::with_capacity(512);
    for piece in [
        b"{\"id\":10,\"run_id\":7,\"attempt\":3,\"steps\":[{\"number\":0,\"name\":\"Set up job\",\"status\":\""
            .as_slice(),
        statuses[0].as_bytes(),
        b"\"},{\"number\":1,\"name\":\"build\",\"status\":\"",
        statuses[1].as_bytes(),
        b"\"},{\"number\":2,\"name\":\"Complete job\",\"status\":\"",
        statuses[2].as_bytes(),
        b"\"}]}",
    ] {
        for byte in piece {
            out.push(*byte).expect("Fixture room");
        }
    }
    out.into_boxed()
}
fn read_job(document: &[u8], log: &[u8], max: u32) -> fixtures::Harness {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::Job { attempt: ATTEMPT, max_bytes: max }));
    h.answer(fixtures::FORGE, &fixtures::ok(RUN));
    h.answer(fixtures::FORGE, &fixtures::ok(document));
    h.answer(fixtures::FORGE, &fixtures::ok(log));
    h.run(Duration::from_millis(10));
    h
}
#[test]
fn the_failed_step_is_the_first_whose_status_is_failure() {
    type StepRow = (&'static str, [&'static str; 3], u32, &'static [u8], &'static [u8]);
    let rows: [StepRow; 3] = [
        (
            "workflow before completion",
            ["success", "failure", "failure"],
            1,
            b"build",
            b"GET /api/v1/repos/fixture/specimen/actions/jobs/10/logs?attempt=3&step=1 HTTP/1.1",
        ),
        (
            "set-up",
            ["failure", "cancelled", "failure"],
            0,
            b"Set up job",
            b"GET /api/v1/repos/fixture/specimen/actions/jobs/10/logs?attempt=3&step=0 HTTP/1.1",
        ),
        (
            "completion alone",
            ["success", "success", "failure"],
            2,
            b"Complete job",
            b"GET /api/v1/repos/fixture/specimen/actions/jobs/10/logs?attempt=3&step=2 HTTP/1.1",
        ),
    ];
    for (name, statuses, number, step, log) in rows {
        let h = read_job(&job(statuses), b"abc", 4);
        let lines = h.request_lines(fixtures::FORGE).into_boxed();
        assert_eq!(lines[3].as_ref(), b"GET /api/v1/repos/fixture/specimen/actions/jobs/10 HTTP/1.1", "{name}: job");
        assert_eq!(lines[4].as_ref(), log, "{name}: step log");
        let expected = Answer::Job {
            attempt: ATTEMPT,
            step: Some(Step { number, name: Box::from(step) }),
            log: Box::from(&b"abc"[..]),
            truncated: false,
        };
        let answer = h.answered(1).expect("Job answer");
        assert_eq!((answer.cost, answer.result.clone()), (3, Ok(expected)), "{name}");
    }
}
#[test]
fn a_job_with_no_failed_step_reads_its_log_without_a_step() {
    let h = read_job(&job(["success", "cancelled", "cancelled"]), b"abc", 4);
    assert_eq!(
        h.request_lines(fixtures::FORGE).into_boxed()[4].as_ref(),
        b"GET /api/v1/repos/fixture/specimen/actions/jobs/10/logs?attempt=3 HTTP/1.1"
    );
    let expected = Answer::Job { attempt: ATTEMPT, step: None, log: Box::from(&b"abc"[..]), truncated: false };
    let answer = h.answered(1).expect("Job answer");
    assert_eq!((answer.cost, answer.result.clone()), (3, Ok(expected)));
}
#[test]
fn a_step_name_is_read_up_to_string_bytes_and_empty_past_it() {
    for (length, kept) in [(128, 128), (129, 0)] {
        let name = &[b'x'; 129][..length];
        let document = named_job(name);
        let h = read_job(&document, b"abc", 4);
        let expected = Answer::Job {
            attempt: ATTEMPT,
            step: Some(Step { number: 1, name: Box::from(&name[..kept]) }),
            log: Box::from(&b"abc"[..]),
            truncated: false,
        };
        let answer = h.answered(1).expect("Job answer");
        assert_eq!((answer.cost, answer.result.clone()), (3, Ok(expected)), "Length {length}");
    }
}
fn named_job(name: &[u8]) -> Box<[u8]> {
    let mut out = skein_lib::List::with_capacity(512);
    for piece in [
        b"{\"id\":10,\"run_id\":7,\"attempt\":3,\"steps\":[{\"number\":1,\"name\":\"".as_slice(),
        name,
        b"\",\"status\":\"failure\"}]}",
    ] {
        for byte in piece {
            out.push(*byte).expect("Fixture room");
        }
    }
    out.into_boxed()
}
#[test]
fn a_failed_step_after_more_steps_than_inner_is_found() {
    let mut document = skein_lib::List::with_capacity(2048);
    for piece in [b"{\"id\":10,\"run_id\":7,\"attempt\":3,\"steps\":[".as_slice()] {
        for byte in piece {
            document.push(*byte).expect("Fixture room");
        }
    }
    for number in 0..12 {
        if number > 0 {
            document.push(b',').expect("Fixture room");
        }
        for byte in b"{\"number\":" {
            document.push(*byte).expect("Fixture room");
        }
        if number >= 10 {
            document.push(b'1').expect("Fixture room");
        }
        document.push(b'0' + (number % 10)).expect("Fixture room");
        let suffix: &[u8] = if number < 10 {
            b",\"name\":\"build\",\"status\":\"success\"}"
        } else {
            b",\"name\":\"build\",\"status\":\"failure\"}"
        };
        for byte in suffix {
            document.push(*byte).expect("Fixture room");
        }
    }
    for byte in b"]}" {
        document.push(*byte).expect("Fixture room");
    }
    let h = read_job(&document.into_boxed(), b"abc", 4);
    let expected = Answer::Job {
        attempt: ATTEMPT,
        step: Some(Step { number: 10, name: Box::from(&b"build"[..]) }),
        log: Box::from(&b"abc"[..]),
        truncated: false,
    };
    let answer = h.answered(1).expect("Job answer");
    assert_eq!((answer.cost, answer.result.clone()), (3, Ok(expected)));
    assert_eq!(
        h.request_lines(fixtures::FORGE).into_boxed()[4].as_ref(),
        b"GET /api/v1/repos/fixture/specimen/actions/jobs/10/logs?attempt=3&step=10 HTTP/1.1"
    );
}
#[test]
fn a_jobs_log_room_is_answer_bytes_less_its_steps_name() {
    let mut shared = fixtures::shared();
    // The 4096-byte fixture bound replaces the shipped 65536 to keep this test cheap.
    shared.answer_bytes = 4096;
    let max = shared.answer_bytes;
    let log = Box::new([b'x'; 4096]);
    let mut h = fixtures::Harness::with(fixtures::limits(), shared, fixtures::settings());
    h.connected(fixtures::FORGE);
    h.answer(fixtures::FORGE, &fixtures::user_ok());
    h.answer(fixtures::FORGE, &fixtures::settings_ok());
    h.known(fixtures::REPO, fixtures::specimen());
    h.call(1, fixtures::REPO, Op::Read(Read::Job { attempt: ATTEMPT, max_bytes: max }));
    h.answer(fixtures::FORGE, &fixtures::ok(RUN));
    h.answer(fixtures::FORGE, &fixtures::ok(JOB));
    h.answer(fixtures::FORGE, &fixtures::ok(log.as_slice()));
    h.run(Duration::from_millis(10));
    let expected = Answer::Job {
        attempt: ATTEMPT,
        step: Some(Step { number: 1, name: Box::from(&b"build"[..]) }),
        log: Box::from(&log[..log.len() - 5]),
        truncated: true,
    };
    assert_eq!(h.answered(1).expect("Job answer").result, Ok(expected));
}

#[test]
fn an_empty_log_answered_with_an_empty_range_is_an_empty_log() {
    let mut h = fixtures::known();
    h.call(1, fixtures::REPO, Op::Read(Read::Job { attempt: ATTEMPT, max_bytes: 4 }));
    h.answer(fixtures::FORGE, &fixtures::ok(RUN));
    h.answer(fixtures::FORGE, &fixtures::ok(JOB));
    h.answer(
        fixtures::FORGE,
        b"HTTP/1.1 206 Partial Content\r\nContent-Length: 0\r\nContent-Range: bytes 0--1/0\r\n\r\n",
    );
    h.run(Duration::from_millis(10));
    let answer = h.answered(1).expect("Empty log");
    let step = Some(Step { number: 1, name: Box::from(&b"build"[..]) });
    assert_eq!(
        (answer.cost, answer.result.clone()),
        (3, Ok(Answer::Job { attempt: ATTEMPT, step, log: Box::from([]), truncated: false }))
    );
}
