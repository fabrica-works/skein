use super::*;
use crate::api::{Failing, Job, JobRef, Order, Run, Step, StepStatus};

fn failing(reruns: u32) -> (Harness, u64) {
    let mut configured = setup();
    configured.checks.passes = 0;
    configured.checks.reruns = reruns;
    let mut h = Harness::with(CALM, configured);
    let work = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"work", work).unwrap();
    h.settle();
    (h, work)
}

#[test]
fn a_failing_ci_status_links_its_run_number_and_job_index() {
    let mut configured = setup();
    configured.checks.passes = 0;
    configured.checks.contexts = names(&[b"ci", b"fmt"]);
    let mut h = Harness::with(CALM, configured);
    let work = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"work", work).unwrap();
    h.settle();
    let Answer::Checks(checks) = h.ok(PERSON, read(Read::Checks { commit: work })) else { unreachable!("checks") };
    let check = checks.first().unwrap();
    assert_eq!(check.status.state, Check::Failed);
    assert_eq!(&*check.link, b"/ai/kit/actions/runs/1/jobs/0");
    assert_eq!(check.job, Some(JobRef { run: 1, job: 1, attempt: 1 }));
    let second = checks.get(1).unwrap();
    assert_eq!(&*second.link, b"/ai/kit/actions/runs/1/jobs/1");
    assert_eq!(second.job, Some(JobRef { run: 1, job: 2, attempt: 1 }));
    assert_eq!(
        h.ok(PERSON, read(Read::Runs { commit: work, number: 1 })),
        Answer::Runs(Box::new([Run { id: 1, number: 1, commit: work }]))
    );
}

#[test]
fn a_commits_run_is_found_by_its_number_and_its_jobs_by_run() {
    let (mut h, work) = failing(0);
    let expected = Run { id: 1, number: 1, commit: work };
    let rows = [
        (Read::Runs { commit: work, number: 1 }, Ok(Answer::Runs(Box::new([expected])))),
        (Read::Runs { commit: work, number: 2 }, Ok(Answer::Runs(Box::new([])))),
        (Read::Run { run: 1 }, Ok(Answer::Run(expected))),
        (Read::Run { run: 2 }, Err(Error::Missing(What::Job))),
        (Read::Jobs { run: 1 }, Ok(Answer::Jobs(Box::new([Job { id: 1, run: 1, index: 0, attempt: 1 }])))),
        (Read::Jobs { run: 2 }, Err(Error::Missing(What::Job))),
    ];
    for (input, expected) in rows {
        assert_eq!(h.call(PERSON, read(input.clone())), expected, "{input:?}");
        assert_eq!(h.call(STRANGER, read(input.clone())), Err(Error::Forbidden), "{input:?} permission");
    }
    let mut configured = setup();
    configured.id = 3;
    configured.name = copy_of(b"ai/other");
    let first = crate::repository(&mut h.domain, &CALM, configured);
    crate::grant(&mut h.domain, b"ai/other", ENGINE, Permission::Write);
    let next = h.commit(first, &[(b"src", b"two")]);
    h.call_on(b"ai/other", ENGINE, Op::Git(Git::Push { branch: copy_of(b"work"), commit: next, expected: None }))
        .unwrap();
    assert_eq!(
        h.call_on(b"ai/other", ENGINE, read(Read::Run { run: 2 })),
        Ok(Answer::Run(Run { id: 2, number: 1, commit: next }))
    );
    assert_eq!(
        h.call_on(b"ai/other", ENGINE, read(Read::Jobs { run: 2 })),
        Ok(Answer::Jobs(Box::new([Job { id: 2, run: 2, index: 0, attempt: 1 }])))
    );
}

#[test]
fn a_job_log_is_read_by_job_id_and_attempt() {
    let (mut h, _) = failing(0);
    for (job, attempt, expected) in [
        (1, 1, Ok(Answer::File(copy_of(WHOLE_LOG)))),
        (1, 2, Err(Error::Missing(What::Job))),
        (2, 1, Err(Error::Missing(What::Job))),
    ] {
        assert_eq!(
            h.call(PERSON, read(Read::Log { job, attempt, step: None })),
            expected,
            "job {job} attempt {attempt}"
        );
    }
}

#[test]
fn a_rerun_context_answers_its_second_attempt() {
    let (mut h, work) = failing(1000);
    assert_eq!(
        h.ok(PERSON, read(Read::Jobs { run: 1 })),
        Answer::Jobs(Box::new([Job { id: 1, run: 1, index: 0, attempt: 2 }]))
    );
    let Answer::Checks(checks) = h.ok(PERSON, read(Read::Checks { commit: work })) else { unreachable!("checks") };
    assert_eq!(checks.first().unwrap().job, Some(JobRef { run: 1, job: 1, attempt: 2 }));
    assert_eq!(h.call(PERSON, read(Read::Log { job: 1, attempt: 1, step: None })), Err(Error::Missing(What::Job)));
    assert_eq!(h.ok(PERSON, read(Read::Log { job: 1, attempt: 2, step: None })), Answer::File(copy_of(WHOLE_LOG)));
}

#[test]
fn a_thread_is_one_read_oldest_first() {
    let mut h = Harness::new(CALM);
    assert_eq!(h.ok(PERSON, create(b"thread", b"body", &[])), Answer::Created(1));
    let rows = [(b"one".as_slice(), 1), (b"two".as_slice(), 2), (b"three".as_slice(), 3)];
    for (text, id) in rows {
        assert_eq!(
            h.ok(PERSON, write(Write::Comment { number: 1, body: copy_of(text) })),
            Answer::Commented(id),
            "{text:?}"
        );
    }
    let Answer::Comments(comments) = h.ok(ENGINE, read(Read::Comments { number: 1 })) else { unreachable!("thread") };
    assert_eq!(comments.len(), 3);
    for (comment, (text, id)) in comments.iter().zip(rows) {
        assert_eq!((comment.id, &*comment.body), (id, text), "comment {id}");
    }
    assert_eq!(h.call(PERSON, read(Read::Comments { number: 2 })), Err(Error::Missing(What::Item)));
    assert_eq!(h.call(STRANGER, read(Read::Comments { number: 1 })), Err(Error::Forbidden));
}

#[test]
fn reviewers_are_requested_and_removed_by_their_own_writes() {
    let (mut h, _) = failing(0);
    assert_eq!(h.open(b"work"), Ok(Answer::Created(1)));
    for (input, expected, reviewers) in [
        (Write::RequestReviewers { number: 1, reviewers: Box::new([PERSON]) }, Ok(Answer::Done), [PERSON].as_slice()),
        (
            Write::RequestReviewers { number: 1, reviewers: Box::new([PERSON, ADMIN]) },
            Ok(Answer::Done),
            [PERSON, ADMIN].as_slice(),
        ),
        (
            Write::RequestReviewers { number: 1, reviewers: Box::new([ENGINE]) },
            Err(Error::Forbidden),
            [PERSON, ADMIN].as_slice(),
        ),
        (
            Write::RemoveReviewers { number: 1, reviewers: Box::new([STRANGER]) },
            Err(Error::Forbidden),
            [PERSON, ADMIN].as_slice(),
        ),
        (Write::RemoveReviewers { number: 1, reviewers: Box::new([PERSON]) }, Ok(Answer::Done), [ADMIN].as_slice()),
    ] {
        assert_eq!(h.call(PERSON, write(input.clone())), Err(Error::Forbidden), "{input:?} permission");
        assert_eq!(h.call(ENGINE, write(input.clone())), expected, "{input:?}");
        assert_eq!(&*h.pull(1).reviewers, reviewers, "{input:?} reviewers");
    }
}

#[test]
fn items_come_newest_created_first_in_forgejos_default_order() {
    let mut h = Harness::new(CALM);
    for title in [b"one".as_slice(), b"two", b"three"] {
        h.ok(PERSON, create(title, b"body", &[]));
    }
    h.ok(PERSON, write(Write::Comment { number: 1, body: copy_of(b"updated") }));
    for (order, expected) in [(Order::NewestCreated, [3, 2, 1]), (Order::LeastUpdated, [2, 3, 1])] {
        let mut found = List::with_capacity(LIMITS.items);
        for page in 1..=2 {
            let input = Read::Items {
                order,
                state: None,
                kind: None,
                labels: Box::new([]),
                author: None,
                since: Time::ZERO,
                page,
                limit: 2,
            };
            let Answer::Items { items, .. } = h.ok(PERSON, read(input)) else { unreachable!("items") };
            for item in &items {
                found.push(item.number).unwrap();
            }
        }
        assert_eq!(found.as_slice(), expected, "{order:?}");
    }
}

#[test]
fn pull_for_answers_the_open_pull_before_an_older_closed_one() {
    let (mut h, _) = failing(0);
    assert_eq!(h.open(b"work"), Ok(Answer::Created(1)));
    h.ok(ENGINE, write(Write::Close { number: 1 }));
    assert_eq!(h.open(b"work"), Ok(Answer::Created(2)));
    for (input, expected) in [
        (Read::PullFor { head: copy_of(b"work"), base: copy_of(MAIN) }, 2),
        (Read::FirstPull { head: copy_of(b"work"), base: copy_of(MAIN) }, 1),
    ] {
        let Answer::Pull(pull) = h.ok(PERSON, read(input.clone())) else { unreachable!("pull") };
        assert_eq!(pull.number, expected, "{input:?}");
        assert_eq!(pull.merge_base, Some(FIRST), "{input:?} merge base");
    }
    let Answer::Pulls(pulls) = h.ok(PERSON, read(Read::OpenPulls { page: 1, limit: 2 })) else {
        unreachable!("open pulls")
    };
    assert_eq!(pulls.len(), 1);
    assert_eq!(pulls.first().unwrap().number, 2);
    h.ok(ENGINE, write(Write::Close { number: 2 }));
    let Answer::Pull(pull) = h.ok(PERSON, read(Read::PullFor { head: copy_of(b"work"), base: copy_of(MAIN) })) else {
        unreachable!("oldest closed pull")
    };
    assert_eq!(pull.number, 1);
}

#[test]
fn an_empty_pull_opens_only_when_opens_empty() {
    let mut h = Harness::new(CALM);
    h.ok(ENGINE, create_branch(b"work", FIRST));
    for (head, opens_empty, expected) in [
        (b"work".as_slice(), false, Err(Error::NothingToMerge)),
        (b"main".as_slice(), true, Err(Error::NothingToMerge)),
        (b"work".as_slice(), true, Ok(Answer::Created(1))),
    ] {
        let input = Write::OpenPull {
            title: copy_of(b"empty"),
            body: Box::new([]),
            head: copy_of(head),
            base: copy_of(MAIN),
            opens_empty,
        };
        assert_eq!(h.call(ENGINE, write(input)), expected, "head {head:?} opens_empty {opens_empty}");
    }
}

const WORKFLOW_LOG: &[u8] = b"failing check ci 0\nfailing check ci 1\nfailing check ci 2\nfailing check ci 3\n";
const WHOLE_LOG: &[u8] =
    b"set up ci\nfailing check ci 0\nfailing check ci 1\nfailing check ci 2\nfailing check ci 3\njob ci failed\n";

#[test]
fn a_failed_job_read_by_id_has_three_steps_failing_at_the_workflow() {
    let (mut h, _) = failing(0);
    assert_eq!(
        h.ok(PERSON, read(Read::Job { job: 1 })),
        Answer::Job {
            job: Job { id: 1, run: 1, index: 0, attempt: 1 },
            steps: Box::new([
                Step {
                    number: 0,
                    name: copy_of(b"Set up job"),
                    status: StepStatus::Success,
                    log: copy_of(b"set up ci\n")
                },
                Step { number: 1, name: copy_of(b"Run ci"), status: StepStatus::Failure, log: copy_of(WORKFLOW_LOG) },
                Step {
                    number: 2,
                    name: copy_of(b"Complete job"),
                    status: StepStatus::Failure,
                    log: copy_of(b"job ci failed\n")
                },
            ]),
        }
    );
}

#[test]
fn a_cancelled_job_has_its_workflow_and_completion_steps_cancelled() {
    let (mut h, _) = failing(0);
    crate::set_failing(&mut h.domain, REPOSITORY, b"ci", Failing::NoStep);
    let Answer::Job { steps, .. } = h.ok(PERSON, read(Read::Job { job: 1 })) else { unreachable!("job") };
    assert_eq!(steps[0].status, StepStatus::Success);
    assert_eq!(steps[1].status, StepStatus::Cancelled);
    assert_eq!(steps[2].status, StepStatus::Cancelled);
    assert_eq!(&*steps[2].log, b"job ci cancelled\n");
}

#[test]
fn each_step_of_a_failed_jobs_log_is_a_slice_of_its_own() {
    let (mut h, _) = failing(0);
    for (step, expected) in [
        (Some(0), b"set up ci\n".as_slice()),
        (Some(1), WORKFLOW_LOG),
        (Some(2), b"job ci failed\n"),
        (None, WHOLE_LOG),
    ] {
        assert_eq!(
            h.ok(PERSON, read(Read::Log { job: 1, attempt: 1, step })),
            Answer::File(copy_of(expected)),
            "{step:?}"
        );
    }
}

#[test]
fn a_log_step_past_the_last_is_missing() {
    let (mut h, _) = failing(0);
    assert_eq!(h.call(PERSON, read(Read::Log { job: 1, attempt: 1, step: Some(3) })), Err(Error::Missing(What::Job)));
}

#[test]
fn a_job_read_by_id_answers_its_latest_attempt() {
    let (mut h, _) = failing(1000);
    let Answer::Job { job, .. } = h.ok(PERSON, read(Read::Job { job: 1 })) else { unreachable!("job") };
    assert_eq!(job, Job { id: 1, run: 1, index: 0, attempt: 2 });
}

#[test]
fn a_passing_ci_jobs_read_by_id_is_missing() {
    let mut h = Harness::with(CALM, setup());
    let work = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"work", work).unwrap();
    h.settle();
    assert_eq!(h.call(PERSON, read(Read::Job { job: 1 })), Err(Error::Missing(What::Job)));
}

#[test]
fn forgotten_statuses_remove_the_commits_run() {
    let mut h = Harness::new(Config { limits: Limits { statuses: 1, ..LIMITS }, ..CALM });
    let one = h.commit(FIRST, &[(b"src", b"one")]);
    let two = h.commit(one, &[(b"src", b"two")]);
    h.push(ENGINE, b"work", one).expect("first head");
    h.push(ENGINE, b"work", two).expect("replacement head");
    assert_eq!(
        h.ok(PERSON, read(Read::Runs { commit: one, number: 1 })),
        Answer::Runs(Box::new([])),
        "the displaced commit's run was forgotten"
    );
}
