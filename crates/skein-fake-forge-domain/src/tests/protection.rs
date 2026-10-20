use super::*;
// Admission makes the queue's group branch, whose name needs the longer bound.
const QUEUEING: Config = Config { limits: Limits { branches: 10, name_bytes: 128, ..LIMITS }, ..CALM };
fn protected(queue: bool) -> Setup {
    let mut setup = setup();
    setup.protection = Some(Protection {
        branch: copy_of(MAIN),
        contexts: names(&[b"required"]),
        approvals: 1,
        dismiss_stale: true,
        queue,
        linear: true,
        code_owners: true,
        bypassing_users: Box::from([ENGINE]),
    });
    setup
}
#[test]
fn bypass_skips_unmet_checks_and_approvals_but_keeps_queue_requirement() {
    for queue in [false, true] {
        let mut h = Harness::with(QUEUEING, protected(queue));
        let head = h.commit(FIRST, &[(b"src", b"one")]);
        h.push(ENGINE, b"alice", head).unwrap();
        h.open(b"alice").unwrap();
        assert_eq!(h.call(MAINTAINER, merge(1, head)), Err(Error::Protected));
        if queue {
            assert_eq!(h.call(ENGINE, merge(1, head)), Err(Error::Protected));
            h.ok(ENGINE, write(Write::Enqueue { number: 1, head }));
            assert_eq!(h.domain.queue_entries(REPOSITORY).unwrap().len(), 1);
        } else {
            assert!(matches_merged(&h.call(ENGINE, merge(1, head))));
        }
    }
}
#[test]
fn queue_admission_requires_approvals_before_processing() {
    let mut setup = protected(true);
    setup.protection.as_mut().unwrap().bypassing_users = Box::from([]);
    let mut h = Harness::with(QUEUEING, setup);
    let head = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"alice", head).unwrap();
    h.open(b"alice").unwrap();
    assert_eq!(h.call(ENGINE, write(Write::Enqueue { number: 1, head })), Err(Error::Protected));
    assert!(h.domain.queue_entries(REPOSITORY).unwrap().is_empty());
    h.ok(MAINTAINER, review(1, Verdict::Approve));
    let moved = h.commit(head, &[(b"src", b"two")]);
    h.push(ENGINE, b"alice", moved).unwrap();
    assert_eq!(h.call(ENGINE, write(Write::Enqueue { number: 1, head: moved })), Err(Error::Protected));
    h.ok(MAINTAINER, review(1, Verdict::Approve));
    // Admission checks approvals; checks of the group commit follow admission.
    h.ok(ENGINE, write(Write::Enqueue { number: 1, head: moved }));
    assert_eq!(h.domain.queue_entries(REPOSITORY).unwrap().len(), 1);
}
#[test]
fn protection_of_another_base_does_not_require_approvals_at_admission() {
    let mut setup = protected(true);
    setup.protection.as_mut().unwrap().branch = copy_of(b"release");
    let mut h = Harness::with(QUEUEING, setup);
    let head = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"alice", head).unwrap();
    h.open(b"alice").unwrap();
    h.ok(MAINTAINER, write(Write::Enqueue { number: 1, head }));
}
#[test]
#[should_panic(expected = "bypassing users within the limits")]
fn bypass_list_refuses_one_past_its_bound() {
    let mut setup = protected(false);
    setup.protection.as_mut().unwrap().bypassing_users = Box::from([1, 2, 3, 4, 5]);
    Harness::with(QUEUEING, setup);
}
#[test]
fn bypass_list_at_its_bound_is_kept_and_priced() {
    let mut setup = protected(false);
    setup.protection.as_mut().unwrap().bypassing_users = Box::from([1, 2, 3, 4]);
    let h = Harness::with(QUEUEING, setup);
    let value = h.domain.inspect_protection(REPOSITORY, MAIN).unwrap();
    assert_eq!(value.bypassing_users.as_ref(), &[1, 2, 3, 4]);
    assert!(value.linear && value.code_owners);
    assert!(worst_case(&Limits { bypassing_users: 5, ..LIMITS }).unwrap() >= worst_case(&LIMITS).unwrap() + 16);
}
