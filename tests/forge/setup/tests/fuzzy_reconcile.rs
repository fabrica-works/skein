use skein_forge::forgejo::request::{Grant, Request};
use skein_forge_setup_world::{Ended, fixtures};
#[test]
fn random_initial_forges_converge_and_a_second_run_writes_only_hook_patches() {
    for seed in 0..32 {
        let mut world = fixtures::random(seed);
        let Hooks { url, expected } = seed_hooks(&mut world, seed);
        assert_eq!(world.reconcile(), Ended::Done, "seed {seed}");
        for &(id, lowest) in &expected {
            let hooks: Vec<_> =
                world.store().repository_hooks(u32::try_from(id).expect("small id")).filter(|h| h.url == url).collect();
            assert_eq!(hooks.len(), 1, "seed {seed}, repository {id}");
            if let Some(lowest) = lowest {
                assert_eq!(hooks[0].id, lowest, "seed {seed}, repository {id}");
            }
            assert_eq!(&hooks[0].secret, world.file(skein_forge_setup::item::Item::WebhookSecret).expect("secret"));
            assert!(hooks[0].active);
            assert_eq!(
                hooks[0].events.iter().map(AsRef::as_ref).collect::<Vec<&[u8]>>(),
                skein_forge::forgejo::delivery::SUBSCRIBED
            );
        }
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
        assert_eq!(
            fixtures::write_routes(&world, since),
            vec![skein_fake_forge_protocol::Route::EditRepoHook; expected.len()],
            "seed {seed}"
        );
        let patched: Vec<_> = world.requests()[since..]
            .iter()
            .filter_map(
                |request| {
                    if let Request::EditRepoHook { at, .. } = request { Some(at.name.as_ref()) } else { None }
                },
            )
            .collect();
        let names: Vec<&[u8]> = expected
            .iter()
            .map(|(id, _)| match id {
                1 => b"one".as_slice(),
                2 => b"two",
                3 => b"three",
                _ => unreachable!("known ids"),
            })
            .collect();
        assert_eq!(patched, names, "seed {seed}");
        assert!(world.secrets_absent(), "seed {seed}");
        assert_eq!(fixtures::file_writes(&world), files, "seed {seed}");
    }
}

struct Hooks {
    url: Box<[u8]>,
    expected: Vec<(u64, Option<u64>)>,
}
fn seed_hooks(world: &mut skein_forge_setup_world::World, seed: u64) -> Hooks {
    let mut rng = skein_lib::Rng::new(seed);
    let address =
        std::net::SocketAddr::from(([127, 0, 0, 1], 8000 + u16::try_from(rng.next_u64() % 1000).expect("small port")));
    world.listening(address);
    let url = skein_forge_setup::reconcile::hook_url(address, 1).expect("loopback");
    let ids = world.adopted_ids().to_vec();
    let mut expected = Vec::new();
    for id in ids.iter().copied().filter(|id| *id <= 3) {
        let mut lowest = None;
        for _ in 0..rng.next_u64() % 3 {
            let discarded = world.with_hook(
                u32::try_from(id).expect("small id"),
                b"http://discarded",
                fixtures::secret(b"discarded-secret"),
            );
            world.delete_repository_hook(u32::try_from(id).expect("small id"), discarded);
        }
        for _ in 0..rng.next_u64() % 4 {
            let target = if rng.next_u64() & 1 == 1 { url.as_ref() } else { b"http://another-service/hooks/2" };
            let hook =
                world.with_hook(u32::try_from(id).expect("small id"), target, fixtures::secret(b"stale-hook-secret"));
            if target == url.as_ref() {
                lowest = Some(lowest.map_or(hook, |kept: u64| kept.min(hook)));
            }
        }
        expected.push((id, lowest));
    }
    Hooks { url, expected }
}
