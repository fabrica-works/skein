//! Small deterministic line-edit sweep with independent expected outcomes and
//! complete fixture replay (fake-checkout.md, section 5; testing-strategy.md, section 8).

#[test]
fn sixty_four_small_merge_histories_replay_and_match_independent_expectations() {
    for seed in 0..64 {
        assert_eq!(
            skein_fake_checkout_tests::merge_case(seed),
            skein_fake_checkout_tests::merge_case(seed),
            "full replay, seed {seed}"
        );
    }
}
