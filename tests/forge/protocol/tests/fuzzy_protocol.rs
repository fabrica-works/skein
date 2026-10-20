use skein_fake_forge_protocol::Faults;
use skein_forge::api::{Error, Op, Read};
use skein_forge_machine_world::joint::{self, Joint};
use skein_forge_protocol_world::fixtures;
use skein_lib::{Duration, Time};

#[test]
fn each_call_has_one_answer_under_stream_cuts() {
    for seed in (0..16).chain([26]) {
        let mut world = fixtures::mixed_calls(seed);
        world.finish();
        let (calls, answers) = world.calls_and_answers();
        assert!(calls > 0, "seed {seed}");
        assert_eq!(calls, answers, "seed {seed}");
    }
}

#[test]
fn reported_costs_account_for_served_and_cut_requests() {
    for seed in (0..16).chain([26]) {
        let mut world = fixtures::mixed_calls(seed);
        world.finish();
        let a = world.accounting();
        assert_eq!(a.reported, a.written, "seed {seed}: {a:?}");
        assert_eq!(
            a.served,
            a.written.checked_sub(a.cut_in_head).expect("cuts are written requests"),
            "seed {seed}: {a:?}"
        );
    }

    let mut joint = Joint::cutting(26, 300, Faults { reset: 0, drop: 0, server_error: 0 });
    joint.run_for(Duration::from_secs(20));
    joint.call(1, joint::REPO, Op::Read(Read::Items { since: Time::ZERO, page: 1, kind: None }));
    joint.run_for(Duration::from_secs(11));
    let answer = joint.answered(1).expect("calibration terminal");
    let requests = joint.request_counts();
    let startup = joint.start_counts();
    assert_eq!(answer.cost, 2);
    assert_eq!(requests.written, 2);
    assert_eq!(joint.tally().total.checked_sub(startup.served), Some(1));
    assert_eq!(requests.cut_in_head, 1);
    assert_eq!(startup.served, 2);
    assert_eq!(answer.result, Err(Error::Timeout));
}

#[test]
fn no_credential_reaches_a_record_request_line_or_fact() {
    for seed in (0..16).chain([26]) {
        let mut world = fixtures::mixed_calls(seed);
        world.finish();
        assert!(world.secrets_absent(), "seed {seed}");
    }
}
