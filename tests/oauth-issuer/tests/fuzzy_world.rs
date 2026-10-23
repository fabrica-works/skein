mod support;
use support::{Story, run};
#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;
#[test]
fn seeded_sign_ins_faults_and_clocks_settle_and_replay() {
    for seed in 0_u64..32 {
        let story = match seed % 3 {
            0 => Story::SignIn,
            1 => Story::Expired,
            _ => Story::AnotherKey,
        };
        assert_eq!(run(seed, story), run(seed, story), "seed {seed}");
    }
}
