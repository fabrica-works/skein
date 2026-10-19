use skein_oauth_accounts::{Ends, Failure};
use skein_oauth_accounts_world::accounts::{self, Fact, Story};
use skein_world::Memory;

#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;

const SEED: u64 = 131;
const ACCOUNT: u32 = 7;

#[test]
fn accounts_added_and_removed_in_every_state_settle_once() {
    let cancelled = Failure::Exchange(skein_oauth::Failure::Cancelled);
    for (story, expected) in [
        (Story::Idle, vec![Fact::Removed(ACCOUNT), Fact::Closed]),
        (Story::Loading, vec![Fact::Failed(Ends::Grant, cancelled), Fact::Removed(ACCOUNT), Fact::Closed]),
        (Story::SigningIn, vec![Fact::Failed(Ends::SignIn, cancelled), Fact::Removed(ACCOUNT), Fact::Closed]),
        (
            Story::Refreshing,
            vec![Fact::Failed(Ends::Grant, cancelled), Fact::Keep(1), Fact::Removed(ACCOUNT), Fact::Closed],
        ),
        (
            Story::Keeping,
            vec![Fact::Keep(1), Fact::Failed(Ends::Grant, cancelled), Fact::Removed(ACCOUNT), Fact::Closed],
        ),
        (
            Story::Holding,
            vec![Fact::Granted(0), Fact::Failed(Ends::Grant, cancelled), Fact::Removed(ACCOUNT), Fact::Closed],
        ),
    ] {
        let (facts, _) = accounts::protocol(SEED, story);
        let outcome = accounts::run(SEED, story, Memory::Checked);
        let simulated = accounts::facts(&outcome.procs);
        assert_eq!((facts.as_slice(), simulated), (expected.as_slice(), expected.as_slice()), "{story:?}");
    }
}

#[test]
fn a_name_reused_after_removed_starts_from_its_keeper() {
    let cancelled = Failure::Exchange(skein_oauth::Failure::Cancelled);
    let expected = vec![
        Fact::Keep(1),
        Fact::Failed(Ends::Grant, cancelled),
        Fact::Removed(ACCOUNT),
        Fact::Granted(1),
        Fact::Failed(Ends::Grant, cancelled),
        Fact::Removed(ACCOUNT),
        Fact::Closed,
    ];
    let (facts, outcome) = accounts::reused(SEED, Memory::Checked);
    assert_eq!(facts, expected, "protocol");
    assert_eq!(accounts::facts(&outcome.procs), expected, "simulated");
}
