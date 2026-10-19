use skein_oauth_accounts::Failure;
use skein_oauth_accounts_world::{
    sign_out::{self, Story},
    world::Fact,
};
use skein_world::Memory;
#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;
#[test]
fn a_sign_out_settles_for_both_keepers() {
    for (seed, (story, revoked)) in
        (101..).zip([(Story::Confirmed, true), (Story::Unconfirmed, false), (Story::NoRevocation, false)])
    {
        let settled = [Fact::SignedOut(revoked), Fact::Closed];
        for private in [false, true] {
            let outcome = sign_out::run(seed, story, private, Memory::Checked);
            assert_eq!(sign_out::facts(&outcome.procs), &settled, "{story:?}");
        }
        assert_eq!(sign_out::protocol(seed, story).0, settled, "{story:?} on streams");
    }
}
#[test]
fn a_sign_out_under_close_and_abort_settles_once() {
    for (story, terminal) in [
        (Story::Close, Fact::SignedOut(true)),
        (Story::Abort, Fact::Failed(Failure::Exchange(skein_oauth::Failure::Cancelled))),
    ] {
        let expected = [terminal, Fact::Closed];
        for private in [false, true] {
            let outcome = sign_out::run(103, story, private, Memory::Checked);
            assert_eq!(sign_out::facts(&outcome.procs), &expected, "{story:?}");
        }
        assert_eq!(sign_out::protocol(103, story).0, expected);
    }
}
