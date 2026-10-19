use skein_lib::Rng;
use skein_oauth_accounts_world::shared::{self, Story};
use skein_world::Memory;

const SEEDS: u64 = 64;

#[test]
fn seeded_lock_answers_settle_with_one_unlock_each_and_replay() {
    let stories = [
        Story::Busy,
        Story::Slow,
        Story::Newer,
        Story::NoRecord,
        Story::Subject,
        Story::Cancelled,
        Story::Removed,
        Story::Aborted,
    ];
    let count = u64::try_from(stories.len()).expect("story count");
    for seed in 0..SEEDS {
        let story = stories[usize::try_from(Rng::new(seed).below(count)).expect("story index")];
        let first = shared::run(seed, story, Memory::Unchecked);
        let second = shared::run(seed, story, Memory::Unchecked);
        assert_eq!(first.trace, second.trace, "{story:?} at seed {seed}");
        assert_eq!(shared::facts(&first.procs), shared::facts(&second.procs));
    }
}
