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
    const SEED: u64 = 111;
    let expected = [Fact::Lock, Fact::Failed(Failure::Busy), Fact::Closed];
    let outcome = shared::run(SEED, Story::Busy, Memory::Checked);
    let protocol = shared::protocol(SEED, Story::Busy);
    let rows = [
        Row { name: "simulated", facts: shared::facts(&outcome.procs), expected: &expected },
        Row { name: "streams", facts: &protocol.0, expected: &expected },
    ];
    for row in rows {
        assert_eq!(row.facts, row.expected, "{}: no lock was taken", row.name);
    }
}
#[test]
fn a_shared_refresh_finds_a_newer_generation_and_exchanges_nothing() {
    const SEED: u64 = 112;
    let expected = [Fact::Lock, Fact::Granted(1), Fact::Unlock, Fact::Closed];
    let outcome = shared::run(SEED, Story::Newer, Memory::Checked);
    let protocol = shared::protocol(SEED, Story::Newer);
    let rows = [
        Row { name: "simulated", facts: shared::facts(&outcome.procs), expected: &expected },
        Row { name: "streams", facts: &protocol.0, expected: &expected },
    ];
    for row in rows {
        assert_eq!(row.facts, row.expected, "{}", row.name);
    }
}
