use skein_forge::api::Read;
use skein_forge_client::Event;
use skein_forge_machine_world::joint::REPO;
use skein_forge_protocol_world::{World, fixtures};
use skein_lib::{Duration, Time, Token};

#[test]
fn github_calls_costs_and_credentials_survive_cuts_renewal_and_replacement() {
    let mut cuts = 0;
    for seed in 0..8 {
        let mut world = World::github(seed, 300);
        world.run_for(Duration::from_secs(20));
        let number = world.person_issue(b"before");
        let _ = world.read(Read::Items { since: Time::ZERO, page: 1, kind: None });
        world.person_edit(number, b"after");
        let _ = world.read(Read::Item { number, after: 0 });
        fixtures::github_close(&mut world, number);
        world.restart();
        let mints = world.github_mints();
        // Tokens keep their shipped one-hour life. Only simulated time advances.
        world.run_for(Duration::from_secs(3600));
        assert!(world.github_mints() > mints, "renewal, seed {seed}");
        // An owner read is outstanding when the new key/access is handed in.
        world.event(Event::Read { owner: Token::new(100), repository: REPO, read: Read::Item { number, after: 0 } });
        world.replace_github_access();
        world.run_for(Duration::from_secs(20));
        let _ = world.read(Read::Items { since: Time::ZERO, page: 1, kind: None });
        world.finish();
        let (calls, answers) = world.calls_and_answers();
        assert!(calls > 0, "seed {seed}");
        assert_eq!(calls, answers, "seed {seed}");
        assert_eq!(world.owner_counts(), (0, 0), "seed {seed}");
        let a = world.accounting();
        assert_eq!(a.reported, a.written, "seed {seed}: {a:?}");
        assert_eq!(a.served + a.cut_in_head, a.written, "seed {seed}: {a:?}");
        cuts += a.cut_in_head;
        assert!(world.secrets_absent(), "seed {seed}");
        let (keys, jwts, tokens) = world.github_credentials();
        assert_eq!(keys, 2, "both handed keys, seed {seed}");
        assert!(jwts >= 2 && tokens >= 4, "renewed credentials, seed {seed}");
        assert_eq!(
            u64::try_from(tokens).expect("token count"),
            world.github_mints(),
            "every minted token retained, seed {seed}"
        );
    }
    assert!(cuts > 0, "sweep includes charged requests cut before a whole head");
}
