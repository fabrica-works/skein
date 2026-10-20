use super::*;
use crate::api::QueueEntry;

#[test]
fn admissions_keep_exact_heads_and_order_without_processing() {
    let mut h = Harness::new(CALM);
    let head = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"alice", head).unwrap();
    h.push(ENGINE, b"bob", head).unwrap();
    h.open(b"alice").unwrap();
    h.open(b"bob").unwrap();
    // Admission order differs from pull numbering.
    for (number, position) in [(2, 1), (1, 2)] {
        assert_eq!(h.pull(number).queued, None);
        assert_eq!(h.call(PERSON, write(Write::Enqueue { number, head })), Err(Error::Forbidden));
        assert_eq!(h.call(ENGINE, write(Write::Enqueue { number, head: FIRST })), Err(Error::Stale));
        let Answer::Pull(pull) = h.ok(ENGINE, write(Write::Enqueue { number, head })) else { panic!("admitted pull") };
        assert_eq!(pull.queued, Some(QueueEntry { number, head, position }));
        assert_eq!(
            h.call(ENGINE, write(Write::Enqueue { number, head })),
            Err(Error::AlreadyQueued(QueueEntry { number, head, position }))
        );
    }
    let entries = [QueueEntry { number: 2, head, position: 1 }, QueueEntry { number: 1, head, position: 2 }];
    assert_eq!(h.domain.queue_entries(REPOSITORY).unwrap(), entries);
    let moved = h.commit(head, &[(b"src", b"two")]);
    h.push(ENGINE, b"alice", moved).unwrap();
    assert_eq!(h.call(ENGINE, write(Write::Enqueue { number: 1, head })), Err(Error::Stale));
    h.settle();
    assert_eq!(h.domain.queue_entries(REPOSITORY).unwrap(), entries);
    assert_eq!(h.pull(1).state, State::Open);
    assert_eq!(h.pull(1).merged, None);
    assert_eq!(h.branch(MAIN), Some(FIRST));
}

#[test]
fn a_queue_rule_refuses_direct_merges_only_on_its_base() {
    for (queue, branch, blocked) in [(true, MAIN, true), (false, MAIN, false), (true, b"release", false)] {
        let mut setup = setup();
        setup.protection = Some(Protection {
            queue,
            branch: copy_of(branch),
            contexts: names(&[]),
            approvals: 0,
            dismiss_stale: false,
        });
        let mut h = Harness::with(CALM, setup);
        let head = h.commit(FIRST, &[(b"src", b"one")]);
        h.push(ENGINE, b"alice", head).unwrap();
        h.open(b"alice").unwrap();
        assert_eq!(h.call(ENGINE, merge(1, FIRST)), Err(Error::Stale));
        let result = h.call(ENGINE, merge(1, head));
        if blocked {
            assert_eq!(result, Err(Error::Protected));
            assert_eq!(h.branch(MAIN), Some(FIRST));
        } else {
            assert!(matches_merged(&result));
        }
    }
}

#[test]
fn admissions_are_bounded_by_the_repository_item_limit() {
    let mut h = Harness::new(CALM);
    let head = h.commit(FIRST, &[(b"src", b"one")]);
    h.push(ENGINE, b"alice", head).unwrap();
    for position in 1..=LIMITS.items {
        let number = u64::from(position);
        h.open(b"alice").unwrap();
        h.ok(ENGINE, write(Write::Enqueue { number, head }));
        h.ok(ENGINE, write(Write::Close { number }));
    }
    assert_eq!(h.domain.queue_entries(REPOSITORY).unwrap().len(), usize::try_from(LIMITS.items).unwrap());
    assert_eq!(h.open(b"alice"), Err(Error::Full));
}
