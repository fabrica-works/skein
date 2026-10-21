use super::*;
use crate::api::QueueEntry;
use skein_lib::Decimal;

#[test]
fn admissions_keep_exact_heads_and_order_while_context_is_silent() {
    let mut h = queue_harness(true);
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
            linear: false,
            code_owners: false,
            bypassing_users: Box::from([]),
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
    let mut h = queue_harness(true);
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

fn queue_harness(silent: bool) -> Harness {
    let mut configured = setup();
    configured.checks.cue = Some(Cue::File { path: copy_of(b"ci"), green: copy_of(b"green") });
    configured.checks.silent = if silent { 1000 } else { 0 };
    configured.protection = Some(Protection {
        linear: false,
        code_owners: false,
        bypassing_users: Box::from([]),
        queue: true,
        branch: copy_of(MAIN),
        contexts: names(&[b"ci"]),
        approvals: 0,
        dismiss_stale: false,
    });
    Harness::with(
        Config {
            limits: Limits { branches: 10, name_bytes: 128, statuses: 16, hooks: 64, observations: 128, ..LIMITS },
            latency_min: Duration::ZERO,
            latency_max: Duration::ZERO,
            ..CALM
        },
        configured,
    )
}

fn admit(h: &mut Harness, branch: &[u8], changes: &[(&[u8], &[u8])]) -> (u64, u64, u64) {
    let head = h.commit(FIRST, changes);
    h.push(ENGINE, branch, head).unwrap();
    let Answer::Created(number) = h.open(branch).unwrap() else { panic!("opened pull") };
    let Answer::Pull(pull) = h.ok(ENGINE, write(Write::Enqueue { number, head })) else { panic!("admitted pull") };
    (number, head, pull.group.unwrap())
}

fn group_branch(h: &Harness, commit: u64) -> Box<[u8]> {
    for (branch, &head) in h.domain.branches(REPOSITORY) {
        if head == commit && branch.starts_with(b"gh-readonly-queue/") {
            return copy_of(branch);
        }
    }
    panic!("group branch")
}

#[test]
fn one_entry_lands_at_its_group_and_deletes_branch() {
    let mut h = queue_harness(false);
    let (number, head, group) = admit(&mut h, b"alice", &[(b"src", b"one")]);
    let branch = group_branch(&h, group);
    assert_eq!(branch.as_ref(), b"gh-readonly-queue/main/pr-1-0000000000000002000000000000000000000000");
    assert_eq!(h.domain.object(group).unwrap().parent, Some(FIRST));
    assert_ne!(group, head);
    h.settle();
    assert_eq!(h.branch(MAIN), Some(group));
    assert_eq!(h.pull(number).merged, Some(group));
    assert_eq!(h.pull(number).state, State::Closed);
    assert_eq!(h.pull(number).queued, None);
    assert_eq!(h.branch(&branch), None);
}

#[test]
fn two_entries_land_in_order_on_their_group_chain() {
    let mut h = queue_harness(false);
    let (first, _, first_group) = admit(&mut h, b"alice", &[(b"alice", b"one")]);
    let (second, _, second_group) = admit(&mut h, b"bob", &[(b"bob", b"two")]);
    assert_eq!(h.domain.object(second_group).unwrap().parent, Some(first_group));
    h.observations();
    h.settle();
    let mut landed = List::with_capacity(2);
    for observation in h.observations().into_boxed() {
        if let Observation::Merged { number, commit, .. } = observation {
            landed.push((number, commit)).unwrap();
        }
    }
    assert_eq!(landed.as_slice(), [(first, first_group), (second, second_group)]);
    assert_eq!(h.branch(MAIN), Some(second_group));
    assert_eq!(h.pull(second).queued, None);
}

#[test]
fn second_group_passing_first_waits_and_latches_its_verdict() {
    let mut h = queue_harness(false);
    let (first, _, first_group) = admit(&mut h, b"alice", &[(b"alice", b"one")]);
    let (second, _, second_group) = admit(&mut h, b"bob", &[(b"bob", b"two")]);
    let id = h.domain.id(REPOSITORY);
    h.domain
        .timers
        .arm(
            crate::domain::Alarm::Check { repository: id, commit: first_group, context: 0 },
            Time::from_nanos(Duration::from_secs(20).as_nanos()),
        )
        .unwrap();
    h.until(Time::from_nanos(Duration::from_secs(5).as_nanos()));
    assert_eq!(h.pull(second).state, State::Open);
    assert_eq!(h.pull(second).queued.unwrap().position, 2);
    h.ok(CI, status(second_group, b"ci", Check::Failed));
    h.until(Time::from_nanos(Duration::from_secs(20).as_nanos()));
    assert_eq!(h.pull(first).merged, Some(first_group));
    assert_eq!(h.pull(second).merged, Some(second_group));
    assert_eq!(h.branch(MAIN), Some(second_group));
}

#[test]
fn first_failure_rebuilds_second_on_base_then_it_lands() {
    let mut h = queue_harness(false);
    let (first, _, first_group) = admit(&mut h, b"alice", &[(b"ci", b"red")]);
    let (second, _, old_group) = admit(&mut h, b"bob", &[(b"bob", b"two")]);
    let old_branch = group_branch(&h, old_group);
    let first_branch = group_branch(&h, first_group);
    h.until(Time::from_nanos(Duration::from_secs(5).as_nanos()));
    let rebuilt = h.pull(second).group.unwrap();
    assert_ne!(rebuilt, old_group);
    assert_eq!(h.domain.object(rebuilt).unwrap().parent, Some(FIRST));
    assert_eq!(h.pull(first).state, State::Open);
    assert_eq!(h.pull(first).dequeued, Some(crate::api::QueueRemoval::CiFailure));
    assert_eq!(h.pull(first).queued, None);
    assert_eq!(h.branch(&first_branch), None);
    assert_eq!(h.domain.branch(REPOSITORY, &old_branch), Some(rebuilt));
    h.settle();
    assert_eq!(h.pull(second).merged, Some(rebuilt));
    assert_eq!(h.branch(MAIN), Some(rebuilt));
}

#[test]
fn conflicting_group_leaves_at_admission_with_merge_conflict() {
    let mut h = queue_harness(true);
    let head = h.commit(FIRST, &[(b"README", b"alice")]);
    h.push(ENGINE, b"alice", head).unwrap();
    h.open(b"alice").unwrap();
    crate::advance(&mut h.domain, &h.env, REPOSITORY, MAIN, b"README", b"bob", ENGINE).unwrap();
    h.ok(ENGINE, write(Write::Enqueue { number: 1, head }));
    let pull = h.pull(1);
    assert_eq!(pull.state, State::Open);
    assert_eq!(pull.dequeued, Some(crate::api::QueueRemoval::MergeConflict));
    assert_eq!(pull.queued, None);
    assert_eq!(pull.group, None);
}

#[test]
fn silent_required_context_keeps_the_group_waiting() {
    let mut h = queue_harness(true);
    let (number, _, group) = admit(&mut h, b"alice", &[(b"src", b"one")]);
    h.settle();
    assert_eq!(h.pull(number).state, State::Open);
    assert_eq!(h.pull(number).group, Some(group));
    assert_eq!(h.pull(number).queued.unwrap().position, 1);
    assert_eq!(h.branch(MAIN), Some(FIRST));
}

#[test]
fn every_required_context_must_pass_before_landing() {
    let mut h = queue_harness(true);
    let id = h.domain.id(REPOSITORY);
    let repository = h.domain.repositories.get_mut(id).unwrap();
    repository.protection.as_mut().unwrap().contexts = names(&[b"ci", b"audit"]);
    let (number, _, group) = admit(&mut h, b"alice", &[(b"src", b"one")]);
    h.ok(CI, status(group, b"ci", Check::Passed));
    h.settle();
    assert_eq!(h.branch(MAIN), Some(FIRST));
    assert_eq!(h.pull(number).queued.unwrap().position, 1);
    h.ok(ENGINE, status(group, b"audit", Check::Passed));
    assert_eq!(h.branch(MAIN), Some(group));
    assert_eq!(h.pull(number).merged, Some(group));
}

#[test]
fn ci_rerun_after_landing_keeps_the_merge_fact() {
    let mut h = queue_harness(false);
    let id = h.domain.id(REPOSITORY);
    h.domain.repositories.get_mut(id).unwrap().checks.reruns = 1000;
    let (number, _, group) = admit(&mut h, b"alice", &[(b"src", b"one")]);
    h.settle();
    h.ok(CI, status(group, b"ci", Check::Failed));
    assert_eq!(h.pull(number).merged, Some(group));
    assert_eq!(h.branch(MAIN), Some(group));
    assert_eq!(h.pull(number).dequeued, None);
    let mut merges = 0_u32;
    for o in h.observations().into_boxed() {
        if let Observation::Merged { .. } = o {
            merges += 1;
        }
    }
    assert_eq!(merges, 1);
}

#[test]
fn full_store_at_rebuild_keeps_entry_waiting_and_records_refusal() {
    let mut h = queue_harness(true);
    let (first, _, first_group) = admit(&mut h, b"alice", &[(b"alice", b"one")]);
    let (second, _, _) = admit(&mut h, b"bob", &[(b"bob", b"two")]);
    let config = h.env.limits;
    for _ in 0..config.limits.commits {
        if crate::commit_tree(&mut h.domain, &config, FIRST, None, files(&[]), b"") == Err(Error::Full) {
            break;
        }
    }
    h.observations();
    h.ok(CI, status(first_group, b"ci", Check::Failed));
    assert_eq!(h.pull(first).dequeued, Some(crate::api::QueueRemoval::CiFailure));
    assert_eq!(h.pull(second).group, None);
    assert_eq!(h.pull(second).queued.unwrap().position, 1);
    assert_eq!(h.domain.room().commits, 0);
    assert!(h.domain.tally().full > 0);
    let mut refused = false;
    for o in h.observations().into_boxed() {
        if let Observation::Refused { number: Some(2), error: Error::Full, .. } = o {
            refused = true;
        }
    }
    assert!(refused);
}

#[test]
fn landing_and_ejection_emit_pull_and_group_hooks_and_observations() {
    for (content, landed, reason) in
        [(b"green".as_slice(), true, None), (b"red".as_slice(), false, Some(crate::api::QueueRemoval::CiFailure))]
    {
        let mut h = queue_harness(false);
        let (number, _, group) = admit(&mut h, b"alice", &[(b"ci", content), (b"src", b"one")]);
        h.settle();
        let mut group_hooks = 0_u32;
        let mut pull_hooks = 0_u32;
        let mut enqueued = false;
        let mut dequeued = None;
        while let Some(hook) = h.hooks.pop() {
            if let Request::Hook { change: Change::Group { ended }, commit: Some(commit), .. } = hook {
                assert_eq!(commit, group);
                assert_eq!(ended, group_hooks == 1);
                group_hooks += 1;
            } else if let Request::Hook { change: Change::Pull, number: Some(item), queue, .. } = hook {
                assert_eq!(item, number);
                pull_hooks += 1;
                match queue {
                    None => {}
                    Some(crate::api::QueueHook::Enqueued) => enqueued = true,
                    Some(crate::api::QueueHook::Dequeued(reason)) => dequeued = Some(reason),
                }
            }
        }
        assert!(enqueued);
        assert_eq!(dequeued, reason);
        let mut groups = List::with_capacity(2);
        let mut removed = None;
        let mut merged = None;
        for o in h.observations().into_boxed() {
            if let Observation::Group { number: item, commit, ended, landed, .. } = o {
                groups.push((item, commit, ended, landed)).unwrap();
            } else if let Observation::Dequeued { number: item, reason, .. } = o {
                removed = Some((item, reason));
            } else if let Observation::Merged { number: item, commit, .. } = o {
                merged = Some((item, commit));
            }
        }
        assert_eq!(groups.as_slice(), [(number, group, false, false), (number, group, true, landed)]);
        if let Some(reason) = reason {
            assert_eq!(removed, Some((number, reason)));
        } else {
            assert_eq!(removed, None);
        }
        assert_eq!(merged, if landed { Some((number, group)) } else { None });
        assert_eq!(group_hooks, 2);
        // Opening, admission and leaving each signal the pull.
        assert_eq!(pull_hooks, 3);
    }
}

#[test]
fn generated_group_names_obey_both_sides_of_name_bound() {
    // gh-readonly-queue/ (18), main (4), /pr- (4), 1 (1), - (1), SHA-1 (40).
    const GROUP_NAME_BYTES: u32 = 68;
    for (limit, succeeds) in [(GROUP_NAME_BYTES, true), (GROUP_NAME_BYTES - 1, false)] {
        let mut h = queue_harness(true);
        h.env.limits.limits.name_bytes = limit;
        let head = h.commit(FIRST, &[(b"src", b"one")]);
        h.push(ENGINE, b"alice", head).unwrap();
        h.open(b"alice").unwrap();
        let result = h.call(ENGINE, write(Write::Enqueue { number: 1, head }));
        if succeeds {
            assert!(result.is_ok());
            assert!(h.pull(1).group.is_some());
        } else {
            assert_eq!(result, Err(Error::TooLarge));
            assert_eq!(h.pull(1).queued, None);
        }
    }
}

#[test]
fn group_branches_use_the_last_branch_slot_then_refuse_admission() {
    let mut h = queue_harness(true);
    let head = h.commit(FIRST, &[(b"bob", b"two")]);
    h.push(ENGINE, b"alice", head).unwrap();
    h.open(b"alice").unwrap();
    h.push(ENGINE, b"bob", head).unwrap();
    h.open(b"bob").unwrap();
    let capacity = h.env.limits.limits.branches;
    for index in h.domain.branches(REPOSITORY).len()..capacity.checked_sub(1).unwrap() {
        let branch = Decimal::of(u64::from(index));
        h.ok(ENGINE, create_branch(branch.as_bytes(), FIRST));
    }
    h.ok(ENGINE, write(Write::Enqueue { number: 1, head }));
    assert_eq!(h.call(ENGINE, write(Write::Enqueue { number: 2, head })), Err(Error::Full));
    assert_eq!(h.domain.branches(REPOSITORY).len(), capacity);
    assert!(h.pull(1).group.is_some());
    assert_eq!(h.pull(2).queued, None);
}

#[test]
fn removal_reason_survives_reads_until_another_admission() {
    let mut h = queue_harness(true);
    let (number, head, group) = admit(&mut h, b"alice", &[(b"src", b"one")]);
    h.ok(CI, status(group, b"ci", Check::Failed));
    assert_eq!(h.pull(number).dequeued.unwrap().github(), b"CI_FAILURE");
    h.ok(ENGINE, write(Write::Enqueue { number, head }));
    assert_eq!(h.pull(number).dequeued, None);
    assert_eq!(crate::api::QueueRemoval::MergeConflict.github(), b"MERGE_CONFLICT");
}

#[test]
fn squash_group_message_uses_one_commit_or_the_multi_commit_pull_title() {
    for (commits, expected, dequeued) in [(1_u32, b"green head".as_slice(), false), (2, b"FAIL title".as_slice(), true)]
    {
        let mut h = queue_harness(false);
        let id = h.domain.id(REPOSITORY);
        h.domain.repositories.get_mut(id).unwrap().checks.cue = Some(Cue::Message { fail: copy_of(b"FAIL") });
        let mut head = FIRST;
        for count in 0..commits {
            head = crate::commit(
                &mut h.domain,
                &h.env.limits,
                head,
                files(&[(b"src", if count == 0 { b"one" } else { b"two" })]),
                b"green head",
            )
            .unwrap()
            .unwrap();
        }
        h.push(ENGINE, b"alice", head).unwrap();
        h.open(b"alice").unwrap();
        h.ok(ENGINE, write(Write::EditItem { number: 1, title: Some(copy_of(b"FAIL title")), body: None }));
        h.settle();
        let Answer::Statuses(statuses) = h.ok(ENGINE, Op::Read(Read::Statuses { commit: head })) else {
            panic!("head statuses")
        };
        assert_eq!(statuses.first().unwrap().state, Check::Passed);
        h.ok(ENGINE, write(Write::Enqueue { number: 1, head }));
        let group = h.pull(1).group.unwrap();
        assert_eq!(h.domain.object(group).unwrap().message.as_ref(), expected);
        h.settle();
        let pull = h.pull(1);
        assert_eq!(pull.dequeued, if dequeued { Some(crate::api::QueueRemoval::CiFailure) } else { None });
        assert_eq!(pull.merged, if dequeued { None } else { Some(group) });
        let Answer::Statuses(statuses) = h.ok(ENGINE, Op::Read(Read::Statuses { commit: group })) else {
            panic!("group statuses")
        };
        assert_eq!(statuses.first().unwrap().state, if dequeued { Check::Failed } else { Check::Passed });
    }
}
