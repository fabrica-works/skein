mod support;
use skein_oauth::{Check, Failure};
use support::{Story, run};
#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;
#[test]
fn sign_in_keeps_the_person_and_userinfo_names_the_same_subject() {
    let outcome = run(7, Story::SignIn);
    assert_eq!(outcome.failure, None);
    assert_eq!(outcome.subject.as_deref(), Some(b"person-42".as_slice()));
}
#[test]
fn an_id_token_delivered_past_expiry_and_skew_is_refused() {
    assert_eq!(run(11, Story::Expired).failure, Some(Failure::Unverified { check: Check::Expiry }));
}
#[test]
fn another_published_keys_modulus_fails_the_signature() {
    assert_eq!(run(13, Story::AnotherKey).failure, Some(Failure::Unverified { check: Check::Signature }));
}
#[test]
fn each_story_replays_its_trace_and_state_digest() {
    for story in [Story::SignIn, Story::Expired, Story::AnotherKey] {
        assert_eq!(run(29, story), run(29, story), "{story:?}");
    }
}
