//! Public-boundary QA for oauth.md, sections 3, 4 and 6.5.

use skein_oauth as oauth;
use skein_oauth_accounts::{Failure, Refusal, STATE_REFUSALS};
use skein_oauth_accounts_world::sign_in::{self, Fact, Story};
use skein_world::Memory;

const SEEDS: [u64; 3] = [0, 7, 971];

// The stories the protocol tier scripts; the others run in the simulated world only.
fn on_streams(story: Story) -> bool {
    matches!(
        story,
        Story::Paste | Story::PasteThenListener | Story::ListenerThenPaste | Story::RequiredScopes | Story::Restart
    )
}

fn runs(story: Story) -> Vec<Vec<Fact>> {
    let mut results = Vec::new();
    for seed in SEEDS {
        let protocol = on_streams(story).then(|| sign_in::protocol(seed, story));
        if let Some(protocol) = &protocol {
            assert_eq!(*protocol, sign_in::protocol(seed, story), "protocol replay: {story:?}, seed {seed}");
        }
        for large in [false, true] {
            let first = sign_in::run(seed, story, false, large, Memory::Unchecked);
            let second = sign_in::run(seed, story, false, large, Memory::Unchecked);
            sign_in::assert_replay(&first, &second);
            let facts = sign_in::facts(&first.procs);
            assert_eq!(facts.last(), Some(&Fact::Closed), "{story:?}, seed {seed}");
            if let Some(protocol) = &protocol {
                assert_eq!(facts, protocol.0, "protocol and simulated outcomes: {story:?}, seed {seed}");
            }
            results.push(facts.to_vec());
        }
    }
    results
}

fn successes(facts: &[Fact]) -> usize {
    facts.iter().filter(|fact| matches!(fact, Fact::SignedIn(_))).count()
}

fn taken(facts: &[Fact]) -> usize {
    facts.iter().filter(|fact| matches!(fact, Fact::Taken)).count()
}

fn successful(story: Story, expected_taken: usize, refusal: Option<Refusal>) {
    for facts in runs(story) {
        assert_eq!(successes(&facts), 1, "{story:?}: {facts:?}");
        assert_eq!(taken(&facts), expected_taken, "{story:?}: {facts:?}");
        assert!(!facts.iter().any(|fact| matches!(fact, Fact::Failed(_))), "{story:?}: {facts:?}");
        if let Some(why) = refusal {
            assert!(facts.contains(&Fact::Refused(why)), "{story:?}: {facts:?}");
        }
    }
}

#[test]
fn a_pasted_public_redirect_completes_one_sign_in() {
    successful(Story::Paste, 1, None);
}

#[test]
fn a_paste_wins_over_an_accepted_listener_connection() {
    successful(Story::PasteThenListener, 1, None);
}

#[test]
fn a_listener_winner_makes_the_later_paste_not_waiting() {
    successful(Story::ListenerThenPaste, 0, Some(Refusal::NotWaiting));
}

#[test]
fn a_second_identical_paste_cannot_complete_another_sign_in() {
    successful(Story::PasteTwice, 1, Some(Refusal::NotWaiting));
}

#[test]
fn malformed_and_mismatched_pastes_leave_the_sign_in_available() {
    successful(Story::PasteWrong, 1, Some(Refusal::Redirect));
}

#[test]
fn four_wrong_paste_states_leave_the_listener_available() {
    for facts in runs(Story::StateRefusals) {
        let refused = facts.iter().filter(|fact| **fact == Fact::Refused(Refusal::State)).count();
        assert_eq!(refused, usize::try_from(STATE_REFUSALS).unwrap(), "{facts:?}");
        assert!(facts.contains(&Fact::Refused(Refusal::Full { bound: STATE_REFUSALS })), "{facts:?}");
        assert_eq!(successes(&facts), 1, "{facts:?}");
        assert_eq!(taken(&facts), 0, "{facts:?}");
    }
}

#[test]
fn declined_required_scopes_complete_sign_in_but_refuse_lending() {
    for facts in runs(Story::RequiredScopes) {
        assert_eq!(successes(&facts), 1, "{facts:?}");
        assert!(facts.iter().any(|fact| matches!(fact, Fact::Missing(scopes) if !scopes.is_empty())), "{facts:?}");
        assert!(facts.contains(&Fact::Failed(Failure::NotGranted)), "{facts:?}");
        assert!(!facts.iter().any(|fact| matches!(fact, Fact::Granted(_))), "{facts:?}");
    }
}

#[test]
fn a_paste_after_the_original_deadline_cannot_sign_in() {
    for facts in runs(Story::PasteExpired) {
        assert!(facts.contains(&Fact::Refused(Refusal::NotWaiting)), "{facts:?}");
        assert!(facts.iter().any(|fact| matches!(fact, Fact::Failed(_))), "{facts:?}");
        assert_eq!(successes(&facts), 0, "{facts:?}");
        assert_eq!(taken(&facts), 0, "{facts:?}");
    }
}

#[test]
fn a_paste_after_cancellation_cannot_revive_the_attempt() {
    for facts in runs(Story::PasteCancelled) {
        assert!(facts.contains(&Fact::Failed(Failure::Exchange(oauth::Failure::Cancelled))), "{facts:?}");
        assert!(facts.contains(&Fact::Refused(Refusal::NotWaiting)), "{facts:?}");
        assert_eq!(successes(&facts), 0, "{facts:?}");
        assert_eq!(taken(&facts), 0, "{facts:?}");
    }
}

#[test]
fn an_admitted_sign_in_accepts_a_paste_during_close() {
    successful(Story::PasteClose, 1, None);
}

#[test]
fn cancelling_and_immediately_restarting_reuses_the_fixed_port() {
    for facts in runs(Story::Restart) {
        assert_eq!(facts.iter().filter(|fact| **fact == Fact::Visit).count(), 2, "{facts:?}");
        assert_eq!(successes(&facts), 1, "{facts:?}");
        assert!(facts.contains(&Fact::Failed(Failure::Exchange(oauth::Failure::Cancelled))), "{facts:?}");
        assert!(!facts.iter().any(|fact| matches!(fact, Fact::Refused(_))), "{facts:?}");
    }
}

#[test]
fn paste_races_and_restart_replay_under_seeded_socket_faults() {
    for story in [
        Story::PasteThenListener,
        Story::ListenerThenPaste,
        Story::PasteTwice,
        Story::ListenerWrongState,
        Story::Restart,
    ] {
        for seed in SEEDS {
            let first = sign_in::run(seed, story, true, false, Memory::Unchecked);
            let second = sign_in::run(seed, story, true, false, Memory::Unchecked);
            sign_in::assert_replay(&first, &second);
            let facts = sign_in::facts(&first.procs);
            assert_eq!(facts.last(), Some(&Fact::Closed), "{story:?}: {facts:?}");
            assert!(successes(facts) <= 1, "{story:?}: {facts:?}");
            assert!(taken(facts) <= 1, "{story:?}: {facts:?}");
        }
    }
}
