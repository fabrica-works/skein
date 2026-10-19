use skein_oauth_accounts::Failure;
use skein_oauth_accounts_world::{
    shared::{self, Story},
    world::Fact,
};
use skein_world::Memory;
#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;
struct Row<'a> {
    name: &'static str,
    facts: &'a [Fact],
    expected: &'a [Fact],
}
#[test]
fn a_busy_owner_lock_fails_the_refresh_as_busy() {
    both_worlds(111, Story::Busy, &[Fact::Lock, Fact::Failed(Failure::Busy), Fact::Closed]);
}
#[test]
fn a_shared_refresh_finds_a_newer_generation_and_exchanges_nothing() {
    both_worlds(112, Story::Newer, &[Fact::Lock, Fact::Granted(1), Fact::Unlock, Fact::Closed]);
}

fn both_worlds(seed: u64, story: Story, expected: &[Fact]) {
    let outcome = shared::run(seed, story, Memory::Checked);
    let protocol = shared::protocol(seed, story);
    let rows = [
        Row { name: "simulated", facts: shared::facts(&outcome.procs), expected },
        Row { name: "streams", facts: &protocol.0, expected },
    ];
    for row in rows {
        assert_eq!(row.facts, row.expected, "{}: {story:?}", row.name);
    }
}
#[test]
fn a_slow_owner_lock_fails_the_write_as_busy() {
    both_worlds(113, Story::Slow, &[Fact::Lock, Fact::Failed(Failure::Busy), Fact::Closed]);
}
#[test]
fn a_lock_answered_after_an_abort_is_unlocked_before_closed() {
    both_worlds(
        114,
        Story::Aborted,
        &[Fact::Lock, Fact::Failed(Failure::Exchange(skein_oauth::Failure::Cancelled)), Fact::Unlock, Fact::Closed],
    );
}
#[test]
fn another_process_signing_out_leaves_no_record() {
    both_worlds(115, Story::NoRecord, &[Fact::Lock, Fact::Failed(Failure::Expired), Fact::Unlock, Fact::Closed]);
}
#[test]
fn another_subject_under_the_lock_drops_the_account() {
    both_worlds(
        116,
        Story::Subject,
        &[
            Fact::Lock,
            Fact::Failed(Failure::AnotherSubject),
            Fact::Unlock,
            Fact::Failed(Failure::Expired),
            Fact::Closed,
        ],
    );
}
