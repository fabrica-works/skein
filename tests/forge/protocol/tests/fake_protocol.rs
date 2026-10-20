use skein_fake_forge_protocol::Route;
use skein_forge::api::{Answer, Error, Op, Read, Write};
use skein_forge_machine_world::joint::{Joint, REPO};
use skein_lib::{Duration, Time};
#[test]
fn the_component_starts_and_lists_items_through_the_fakes_bytes() {
    let mut joint = Joint::new(3);
    joint.issue(b"first", b"body");
    joint.run_for(Duration::from_secs(1));
    joint.call(1, REPO, Op::Read(Read::Items { since: Time::ZERO, page: 1, kind: None }));
    joint.run_for(Duration::from_secs(1));
    assert!(joint.answered(1).is_some(), "tally {:?}", joint.tally());
    let answered = joint.answered(1).expect("one answer");
    assert_eq!(answered.cost, 2, "repository resolved, then listed");
    let Ok(Answer::Items { items, more: false, .. }) = &answered.result else {
        panic!("one page: {:?}", answered.result)
    };
    assert_eq!(items.len(), 1);
    assert_eq!(&*items[0].title, b"first");
    assert_eq!(joint.tally().total, 4, "start two reads, then call two");
}
#[test]
fn pull_for_answers_the_open_pull_when_the_lookup_names_a_closed_one() {
    let mut joint = Joint::new(3);
    joint.push(b"work", b"change");
    let first = joint.pull(b"work", b"main");
    joint.close(first);
    let second = joint.pull(b"work", b"main");
    joint.run_for(Duration::from_secs(1));
    joint.call(
        1,
        REPO,
        Op::Read(Read::PullFor { head: Box::from(b"work".as_slice()), base: Box::from(b"main".as_slice()) }),
    );
    joint.run_for(Duration::from_secs(1));
    let answered = joint.answered(1).expect("answer");
    assert_eq!(answered.cost, 4, "resolve, oldest lookup, open page, status");
    let Ok(Answer::Pull(pull)) = &answered.result else { panic!("pull: {:?}", answered.result) };
    assert_eq!(pull.number, second);
    assert_eq!(joint.tally().routes.get(&Route::PullFor), Some(&1));
    assert_eq!(joint.tally().routes.get(&Route::Pulls), Some(&1));
}
#[test]
fn opening_with_nothing_to_merge_writes_no_pull_request() {
    let mut joint = Joint::new(3);
    joint.branch(b"same", 1);
    joint.run_for(Duration::from_secs(1));
    joint.call(
        1,
        REPO,
        Op::Write(Write::OpenPull {
            title: Box::from(b"empty".as_slice()),
            body: Box::from(b"body".as_slice()),
            head: Box::from(b"same".as_slice()),
            base: Box::from(b"main".as_slice()),
        }),
    );
    joint.run_for(Duration::from_secs(1));
    let answered = joint.answered(1).expect("answer");
    assert_eq!(answered.result, Err(Error::NothingToMerge));
    assert_eq!(answered.cost, 4, "resolve, two branches, compare");
    assert_eq!(joint.tally().routes.get(&Route::OpenPull), None);
    let pulls = joint
        .direct(skein_fake_forge_domain::api::Op::Read(skein_fake_forge_domain::api::Read::OpenPulls {
            page: 1,
            limit: 2,
        }))
        .expect("pulls");
    assert_eq!(pulls, skein_fake_forge_domain::api::Answer::Pulls(Box::from([])));
}

const BUILD_STEP_NAME: &[u8] = b"Run build";
const BUILD_STEP_LOG: &[u8] =
    b"failing check build 0\nfailing check build 1\nfailing check build 2\nfailing check build 3\n";
#[test]
fn a_failed_job_read_names_its_failed_step_and_reads_its_log() {
    let mut joint = Joint::failing(5);
    let pushed = joint.push(b"work", b"change");
    joint.run_for(Duration::from_secs(1));
    let head = skein_forge_client_world::translate::commit(pushed);
    joint.call(1, REPO, Op::Read(Read::Checks { commit: head }));
    joint.run_for(Duration::from_secs(1));
    let Some(Ok(Answer::Checks(statuses))) = joint.answered(1).map(|a| &a.result) else {
        panic!("Checks: {:?}", joint.answered(1))
    };
    let attempt = statuses[0].job.expect("The failed status links its job");
    joint.call(2, REPO, Op::Read(Read::Job { attempt, max_bytes: 4096 }));
    joint.run_for(Duration::from_secs(1));
    let answered = joint.answered(2).expect("Job answer");
    assert_eq!(answered.cost, 3, "Run, job, the step log");
    let step = Some(skein_forge::api::Step { number: 1, name: Box::from(BUILD_STEP_NAME) });
    assert_eq!(answered.result, Ok(Answer::Job { attempt, step, log: Box::from(BUILD_STEP_LOG), truncated: false }));
}

#[test]
fn a_failed_jobs_log_end_comes_through_a_suffix_range() {
    let mut joint = Joint::failing(5);
    let pushed = joint.push(b"work", b"change");
    joint.run_for(Duration::from_secs(1));
    let head = skein_forge_client_world::translate::commit(pushed);
    joint.call(1, REPO, Op::Read(Read::Checks { commit: head }));
    joint.run_for(Duration::from_secs(1));
    let Some(Ok(Answer::Checks(statuses))) = joint.answered(1).map(|a| &a.result) else {
        panic!("Checks: {:?}", joint.answered(1))
    };
    let attempt = statuses[0].job.expect("The failed status links its job");
    joint.call(2, REPO, Op::Read(Read::Job { attempt, max_bytes: 8 }));
    joint.run_for(Duration::from_secs(1));
    let answered = joint.answered(2).expect("Job answer");
    assert_eq!(answered.cost, 3, "Run, job, the step log");
    let step = Some(skein_forge::api::Step { number: 1, name: Box::from(BUILD_STEP_NAME) });
    assert_eq!(answered.result, Ok(Answer::Job { attempt, step, log: Box::from(&b"build 3\n"[..]), truncated: true }));
}
