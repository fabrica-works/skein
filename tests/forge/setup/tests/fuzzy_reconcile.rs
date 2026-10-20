use skein_forge::forgejo::request::{Grant, Request};
use skein_forge_setup_world::{Ended, fixtures};
#[test]
fn random_initial_forges_converge_and_a_second_run_writes_nothing() {
    for seed in 0..32 {
        let mut world = fixtures::random(seed);
        assert_eq!(world.reconcile(), Ended::Done, "seed {seed}");
        fixtures::assert_bot(&world);
        for org in [b"acme", b"work"] {
            fixtures::assert_org(&world, org);
        }
        for &id in world.adopted_ids() {
            assert!(world.requests().contains(&Request::RepositoryById { id }), "seed {seed}, repository {id}");
            let name = match id {
                1 => b"one".as_slice(),
                2 => b"two",
                3 => b"three",
                4 => continue,
                _ => unreachable!("scenario repositories"),
            };
            let at = world
                .requests()
                .iter()
                .find_map(|request| {
                    if let Request::Permission { at, .. } = request
                        && at.name.as_ref() == name
                    {
                        Some(at)
                    } else {
                        None
                    }
                })
                .expect("every adopted known repository has its permission read");
            let permission = world.store().collaborator(u32::try_from(id).expect("scenario id"), b"kit");
            if at.owner.as_ref() == b"kit" {
                assert_eq!(permission, None, "seed {seed}, owned repository {id}");
            } else {
                assert!(matches!(permission, Some(Grant::Write | Grant::Admin)), "seed {seed}, repository {id}");
            }
        }
        let since = world.requests().len();
        let files = fixtures::file_writes(&world);
        assert_eq!(world.reconcile(), Ended::Done, "seed {seed}, second run");
        assert!(fixtures::write_routes(&world, since).is_empty(), "seed {seed}");
        assert!(world.secrets_absent(), "seed {seed}");
        assert_eq!(fixtures::file_writes(&world), files, "seed {seed}");
    }
}
