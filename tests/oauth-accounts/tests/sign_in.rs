use skein_oauth_accounts_world::sign_in::{self, Story};
use skein_world::Memory;
#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;
#[test]
fn a_public_client_signs_in_through_its_loopback_listener() {
    drop(sign_in::run(43, Story::Public, false, false, Memory::Checked));
}
#[test]
fn a_confidential_client_signs_in_through_the_owners_redirect() {
    drop(sign_in::run(47, Story::Confidential, true, false, Memory::Checked));
}
#[test]
fn a_wrong_path_is_refused_and_the_sign_in_still_waits() {
    for story in [Story::WrongPath, Story::WrongHost, Story::LongHead] {
        drop(sign_in::run(49, story, true, false, Memory::Checked));
    }
}
#[test]
fn a_close_while_a_sign_in_waits_for_a_person_still_waits_for_it() {
    for story in [Story::CloseWaiting, Story::CloseConfidential] {
        drop(sign_in::run(51, story, false, false, Memory::Checked));
    }
}
#[test]
fn a_sign_in_the_owner_cancels_fails_as_cancelled_and_closes_its_listener() {
    for story in [Story::Cancel, Story::Abort] {
        drop(sign_in::run(53, story, false, false, Memory::Checked));
    }
}
#[test]
fn the_shipped_person_deadline_ends_the_sign_in() {
    drop(sign_in::run(55, Story::Timeout, false, false, Memory::Checked));
}
#[test]
fn a_sign_in_whose_record_is_not_kept_fails_once_and_lends_nothing() {
    drop(sign_in::run(57, Story::NotKept, true, false, Memory::Checked));
}
#[test]
fn sign_in_redirect_and_refresh_run_in_separate_processes_and_replay() {
    let first = sign_in::run(59, Story::Public, true, false, Memory::Checked);
    let second = sign_in::run(59, Story::Public, true, false, Memory::Checked);
    sign_in::assert_replay(&first, &second);
    assert_clients_settle_before_issuer(&first);
    assert_eq!(first.heap, second.heap);
}
#[test]
fn full_issuer_tokens_plans_codes_and_rotations_fit_the_process_heap() {
    drop(sign_in::run(61, Story::Public, false, true, Memory::Checked));
}
#[test]
fn protocol_streams_cover_the_owner_redirect_cancel_close_and_keep_terminals() {
    for story in [
        Story::Public,
        Story::VaryingTaken,
        Story::TwoPorts,
        Story::Confidential,
        Story::WrongPath,
        Story::WrongHost,
        Story::LongHead,
        Story::CloseWaiting,
        Story::CloseConfidential,
        Story::Cancel,
        Story::Abort,
        Story::NotKept,
        Story::Timeout,
    ] {
        let first = sign_in::protocol(67, story);
        let second = sign_in::protocol(67, story);
        assert_eq!(first, second);
    }
}

#[test]
fn a_varying_port_signs_in_with_its_preferred_port_taken() {
    for protocol in [false, true] {
        if protocol {
            drop(sign_in::protocol(71, Story::VaryingTaken));
        } else {
            drop(sign_in::run(71, Story::VaryingTaken, false, false, Memory::Checked));
        }
    }
}
#[test]
fn two_sign_ins_at_once_each_follow_their_own_port() {
    for protocol in [false, true] {
        if protocol {
            drop(sign_in::protocol(73, Story::TwoPorts));
        } else {
            drop(sign_in::run(73, Story::TwoPorts, false, false, Memory::Checked));
        }
    }
}

fn assert_clients_settle_before_issuer(outcome: &skein_world::Outcome<sign_in::Process>) {
    use skein_sim::{Event, Summary};
    let listener = outcome
        .trace
        .iter()
        .find_map(|entry| match entry.event {
            Event::Submit { kind: Summary::Listen { fd, .. }, .. } if entry.pid.raw() == 0 => Some(fd),
            Event::Submit { .. } | Event::Complete { .. } | Event::Fault(_) | Event::Digest(_) => None,
        })
        .expect("the issuer listened");
    let clients_closed = outcome
        .trace
        .iter()
        .enumerate()
        .filter_map(|(at, entry)| {
            (entry.pid.raw() != 0
                && matches!(entry.event, Event::Complete { kind: Summary::Close { .. }, result: Ok(_), .. }))
            .then_some(at)
        })
        .max()
        .expect("the client and browser closed their descriptors");
    let issuer_closed = outcome
        .trace
        .iter()
        .position(|entry| {
            entry.pid.raw() == 0
                && matches!(entry.event,
            Event::Submit { kind: Summary::Close { fd }, .. } if fd == listener)
        })
        .expect("the issuer closed its listener");
    assert!(clients_closed < issuer_closed, "the issuer stays live until all client closes have completed");
}

#[test]
fn required_scopes_declined_then_a_grant_is_refused() {
    for protocol in [false, true] {
        let facts = if protocol {
            sign_in::protocol(79, Story::RequiredScopes).0
        } else {
            let outcome = sign_in::run(79, Story::RequiredScopes, false, false, Memory::Checked);
            sign_in::facts(&outcome.procs).to_vec()
        };
        assert_eq!(
            facts,
            vec![
                sign_in::Fact::Visit,
                sign_in::Fact::Keep(1),
                sign_in::Fact::SignedIn(1),
                sign_in::Fact::Missing(b"write".as_slice().into()),
                sign_in::Fact::Failed(skein_oauth_accounts::Failure::NotGranted),
                sign_in::Fact::Closed,
            ]
        );
    }
}
