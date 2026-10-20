use skein_forge_setup::{
    github::manifest::Target,
    host::Plan,
    item::Item,
    reconcile::{Outcome, Stage, Why},
};
use skein_forge_setup_world::{
    Heard, HostWorld,
    fixtures::github_stories::{
        FORGE, HOOK, accept, accepted, closed, done, expire, install, mutations, reconcile, writes,
    },
};
use skein_lib::Rng;

#[test]
fn seeded_github_setups_converge_and_then_write_nothing() {
    for seed in [7, 19, 43, 101] {
        let mut rng = Rng::new(seed);
        let hook = if rng.below(2) == 0 { HOOK } else { b"https://wrong.test/" };
        let mut world =
            HostWorld::github_scenario(seed, &[], skein_fake_forge_github::service::AppFacts::Complete, hook);
        world.forget(Item::Accepted);
        // Invalid states spend no credential and change neither items nor the fake.
        let (expired, _) = install(&mut world, 1);
        expire(&mut world);
        world.run_plan(2, Plan::Manifest { forge: FORGE, target: Target::Personal });
        world.settle();
        let Some(Heard::Done { outcome: Outcome::Manifest { state: wrong, .. }, .. }) = world.heard().last() else {
            panic!("seed {seed}: manifest")
        };
        let wrong = wrong.clone();
        let invalid = if rng.below(2) == 0 { [expired, wrong] } else { [wrong, expired] };
        for (index, state) in invalid.iter().enumerate() {
            let before =
                (world.needs().len(), world.github_requests().len(), accepted(&world), world.github_installation_ids());
            accept(&mut world, 3 + u64::try_from(index).expect("two invalid states"), state, 1);
            assert!(
                matches!(world.heard().last(), Some(Heard::Stop { failure, .. }) if failure.step == Stage::State && failure.why == Why::Unknown),
                "seed {seed}"
            );
            assert_eq!(
                (world.needs().len(), world.github_requests().len(), accepted(&world), world.github_installation_ids()),
                before,
                "seed {seed}"
            );
        }
        let mut expected = Vec::new();
        for id in 1..=3 {
            if rng.below(4) == 0 {
                world.github_remove(id);
                continue;
            }
            if rng.below(2) == 0 {
                let (state, _) = install(&mut world, 10 + id * 3);
                accept(&mut world, 11 + id * 3, &state, id);
                expected.push(id);
                let before = (
                    world.needs().len(),
                    world.github_requests().len(),
                    accepted(&world),
                    world.github_installation_ids(),
                );
                accept(&mut world, 12 + id * 3, &state, id);
                assert!(
                    matches!(world.heard().last(), Some(Heard::Stop { failure, .. }) if failure.step == Stage::State && failure.why == Why::Unknown),
                    "seed {seed}: reused"
                );
                assert_eq!(
                    (
                        world.needs().len(),
                        world.github_requests().len(),
                        accepted(&world),
                        world.github_installation_ids()
                    ),
                    before,
                    "seed {seed}: reused"
                );
            }
        }
        reconcile(&mut world, 30);
        done(&world, 30);
        assert_eq!(accepted(&world).as_ref(), expected, "seed {seed}");
        assert_eq!(world.github_installation_ids(), expected, "seed {seed}");
        let (needs, requests) = (world.needs().len(), world.github_requests().len());
        reconcile(&mut world, 31);
        done(&world, 31);
        assert!(writes(&world, needs).is_empty(), "seed {seed}");
        assert!(mutations(&world, requests).is_empty(), "seed {seed}");
        closed(&mut world);
    }
}
