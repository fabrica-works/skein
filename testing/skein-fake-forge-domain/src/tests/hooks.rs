//! Forgejo's deliveries carry item bodies and terminal jobs, never statuses.
use super::*;

#[test]
fn a_job_ending_is_heard_with_its_run_job_and_commit() {
    let mut h = Harness::new(CALM);
    let work = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"work", work).expect("pushed");
    h.until(Time::from_nanos(5_000_000_000));
    assert_eq!(h.pull_status(work), Check::Passed, "the status precedes the delivery");
    h.settle();
    let mut ends = List::with_capacity(8);
    for hook in &h.heard_all() {
        match hook {
            Request::Hook { ended: Some(ended), .. } => ends.push(*ended).expect("room for job ends"),
            Request::Hook { ended: None, .. } => {}
            Request::Reply { .. } => unreachable!("only hooks are heard"),
        }
    }
    assert_eq!(
        ends.as_slice(),
        [Ended { job: JobRef { run: 1, job: 1, attempt: 1 }, commit: work, state: Check::Passed }]
    );
}

#[test]
fn a_pending_report_and_a_persons_status_are_not_heard() {
    let mut configured = setup();
    configured.checks.silent = 1000;
    let mut h = Harness::with(CALM, configured);
    let work = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"work", work).expect("pushed");
    h.ok(ENGINE, status(work, b"personal", Check::Passed));
    h.settle();
    assert_eq!(h.heard().as_slice(), [(Change::Push, None)]);
}

#[test]
fn an_issue_a_comment_and_a_review_are_heard_with_their_bodies() {
    let mut h = Harness::new(CALM);
    h.issue(PERSON, b"one");
    h.comment(PERSON, 1, b"comment");
    let work = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"work", work).expect("pushed");
    h.open(b"work").expect("opened");
    h.ok(MAINTAINER, review(2, Verdict::Approve));
    h.settle();
    let mut bodies = List::with_capacity(8);
    for hook in &h.heard_all() {
        match hook {
            Request::Hook { change, body: Some(body), .. } => {
                bodies.push((*change, body.clone())).expect("room for bodies");
            }
            Request::Hook { body: None, .. } => {}
            Request::Reply { .. } => unreachable!("only hooks are heard"),
        }
    }
    assert_eq!(
        bodies.as_slice(),
        [
            (Change::Issue, copy_of(b"body")),
            (Change::Comment, copy_of(b"comment")),
            (Change::Review, copy_of(b"looked"))
        ]
    );
}

#[test]
fn a_pull_reads_not_mergeable_until_the_check_span_passes() {
    let mut h = Harness::new(CALM);
    let work = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"work", work).expect("pushed");
    h.open(b"work").expect("opened");
    let more = h.commit(work, &[(b"src", b"two")]);
    h.push(ENGINE, b"work", more).expect("pushed");
    assert!(!h.pull(1).mergeable);
    h.wait(Duration::from_secs(2));
    h.ok(PERSON, read(Read::Pull { number: 1 }));
    assert!(h.pull(1).mergeable);
}

impl Harness {
    fn pull_status(&self, commit: u64) -> Check {
        let Answer::Statuses(statuses) = self.inspect(&Read::Statuses { commit }).expect("commit status") else {
            panic!("statuses")
        };
        statuses.first().expect("one context").state
    }
}

pub(super) fn branch_observations(work: u64, more: u64, merged: u64) -> Box<[Observation]> {
    Box::new([
        Observation::Defined { repository: repository(), label: copy_of(b"later"), by: ENGINE },
        moved(b"work", None, work),
        pending(work),
        Observation::Opened {
            repository: repository(),
            number: 1,
            kind: Kind::Pull,
            title: copy_of(b"change"),
            body: copy_of(b"body"),
            labels: names(&[]),
            branches: Some(Branches { head: copy_of(b"work"), base: copy_of(MAIN), commit: work }),
            by: ENGINE,
        },
        moved(b"work", Some(work), more),
        pending(more),
        Observation::Reviewed {
            repository: repository(),
            number: 1,
            id: 1,
            commit: more,
            verdict: Verdict::Approve,
            body: copy_of(b"looked"),
            by: MAINTAINER,
        },
        Observation::Merged {
            repository: repository(),
            number: 1,
            base: copy_of(MAIN),
            head: more,
            commit: merged,
            by: ENGINE,
        },
        moved(MAIN, Some(FIRST), merged),
        pending(merged),
        Observation::Deleted { repository: repository(), branch: copy_of(b"work"), at: more, by: ENGINE },
        Observation::Wiki {
            repository: repository(),
            name: copy_of(b"home"),
            content: Some(copy_of(b"notes")),
            revision: 1,
            by: ENGINE,
        },
        Observation::Wiki { repository: repository(), name: copy_of(b"home"), content: None, revision: 2, by: ENGINE },
    ])
}

pub(super) fn issue_observations(h: &mut Harness) -> u64 {
    let number = h.issue(PERSON, b"talk");
    let id = h.comment(PERSON, number, b"oops");
    h.ok(PERSON, write(Write::DeleteComment { id }));
    h.ok(PERSON, write(Write::Close { number }));
    h.ok(PERSON, write(Write::Reopen { number }));
    let observed = h.observations();
    let tail = observed.as_slice().get(2..).expect("the comment and its state");
    let expected = [
        Observation::Removed { repository: repository(), number, id, by: PERSON },
        Observation::Closed { repository: repository(), number, by: PERSON },
        Observation::Reopened { repository: repository(), number, by: PERSON },
    ];
    assert_eq!(tail, expected);
    number
}

pub(super) fn pushed_and_ended(work: u64) -> Box<[Request]> {
    Box::new([
        Request::Hook {
            repository: repository(),
            change: Change::Push,
            number: None,
            branch: Some(copy_of(b"work")),
            commit: Some(work),
            by: Some(ENGINE),
            body: None,
            ended: None,
        },
        Request::Hook {
            repository: repository(),
            change: Change::Job,
            number: None,
            branch: None,
            commit: None,
            by: Some(CI),
            body: None,
            ended: Some(Ended { job: JobRef { run: 1, job: 1, attempt: 1 }, commit: work, state: Check::Passed }),
        },
        Request::Hook {
            repository: repository(),
            change: Change::Push,
            number: None,
            branch: Some(copy_of(b"work")),
            commit: None,
            by: Some(ENGINE),
            body: None,
            ended: None,
        },
    ])
}
