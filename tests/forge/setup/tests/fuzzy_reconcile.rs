use skein_fake_forge_protocol::Route;
use skein_forge_setup_world::{Ended, fixtures};
#[test]
fn random_initial_forges_converge_and_a_second_run_writes_only_hook_patches() {
    for seed in 0..32 {
        let (mut world, other) = fixtures::random(seed);
        assert_eq!(world.reconcile(), Ended::Done, "seed {seed}");
        fixtures::assert_bot(&world);
        for org in [b"acme", b"work"] {
            fixtures::assert_org(&world, org);
        }
        fixtures::assert_other_hooks(&world, &other);
        let since = world.requests().len();
        let files = fixtures::file_writes(&world);
        assert_eq!(world.reconcile(), Ended::Done, "seed {seed}, second run");
        assert_eq!(fixtures::write_routes(&world, since), vec![Route::EditOrgHook, Route::EditOrgHook], "seed {seed}");
        fixtures::assert_other_hooks(&world, &other);
        assert!(world.secrets_absent(), "seed {seed}");
        assert_eq!(fixtures::file_writes(&world), files, "seed {seed}");
    }
}
