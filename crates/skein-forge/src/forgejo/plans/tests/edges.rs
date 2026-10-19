//! Calls that stop before their next exchange.
use crate::api::{Error, Op, Read};
use crate::forgejo::plans::plan::Step;
use crate::forgejo::tests::fixtures::{ATTEMPT, JOB, PULL, RAW_BYTES, RUN, Run};

#[test]
fn pull_files_stops_at_a_stale_head() {
    let mut run = Run::known(Op::Read(Read::PullFiles { number: 1, head: [0; 32], page: 1 }));
    run.begin();
    assert_eq!(run.answer(200, PULL), Step::Done(Err(Error::Stale)));
    run.assert_sent(&["GET /api/v1/repos/fixture/specimen/pulls/1"]);
}

#[test]
fn job_mismatch_is_missing_job() {
    for (attempt, replies, sends) in job_failures() {
        let mut run = Run::known(Op::Read(Read::Job { attempt, max_bytes: RAW_BYTES }));
        let mut step = run.begin();
        for (status, body) in replies {
            step = run.answer(*status, body);
        }
        assert_eq!(step, Step::Done(Err(Error::MissingJob)), "{attempt:?} {replies:?}");
        run.assert_sent(sends);
    }
}

const OTHER_RUN_JOB: &[u8] =
    br#"{"id":10,"run_id":8,"attempt":3,"steps":[{"number":1,"name":"x","status":"failure"}]}"#;
const MISSING: &[u8] = b"{}";
type JobCase = (crate::api::JobAttempt, &'static [(u16, &'static [u8])], &'static [&'static str]);
fn job_failures() -> [JobCase; 6] {
    [
        (
            crate::api::JobAttempt { head: [0; 32], ..ATTEMPT },
            &[(200, RUN)][..],
            &["GET /api/v1/repos/fixture/specimen/actions/runs/7"][..],
        ),
        (
            ATTEMPT,
            &[(200, RUN), (200, OTHER_RUN_JOB)][..],
            &[
                "GET /api/v1/repos/fixture/specimen/actions/runs/7",
                "GET /api/v1/repos/fixture/specimen/actions/jobs/10",
            ][..],
        ),
        (
            crate::api::JobAttempt { attempt: 4, ..ATTEMPT },
            &[(200, RUN), (200, JOB)][..],
            &[
                "GET /api/v1/repos/fixture/specimen/actions/runs/7",
                "GET /api/v1/repos/fixture/specimen/actions/jobs/10",
            ][..],
        ),
        (ATTEMPT, &[(404, MISSING)][..], &["GET /api/v1/repos/fixture/specimen/actions/runs/7"][..]),
        (
            ATTEMPT,
            &[(200, RUN), (404, MISSING)][..],
            &[
                "GET /api/v1/repos/fixture/specimen/actions/runs/7",
                "GET /api/v1/repos/fixture/specimen/actions/jobs/10",
            ][..],
        ),
        (
            ATTEMPT,
            &[(200, RUN), (200, JOB), (404, MISSING)][..],
            &[
                "GET /api/v1/repos/fixture/specimen/actions/runs/7",
                "GET /api/v1/repos/fixture/specimen/actions/jobs/10",
                "GET /api/v1/repos/fixture/specimen/actions/jobs/10/logs?attempt=3&step=1",
            ][..],
        ),
    ]
}
