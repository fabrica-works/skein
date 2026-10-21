use skein_fake_forge_github::keys::KEYS;
use skein_forge::credential::Credential;
use skein_forge_connection::{Access, Event, Request, Settings, Stopped, settings::Kind, start::Start};
use skein_forge_machine_world::{
    fixtures::{self, FORGE},
    machine::{Answer, Machine},
};
use skein_lib::{Duration, Secret};

const SEED: u64 = 7;
const SECOND: Duration = Duration::from_secs(1);
const LISTING: &str = "GET /app/installations?per_page=100&page=1";
const PAGE_TWO: &str = "GET /app/installations?per_page=100&page=2";
const MINT: &str = "POST /app/installations/1/access_tokens";
const VIEWER: &str = "POST /graphql";
// The fake mints an hour of life at WALL_BASE, at the same virtual time as owner action.
const HOUR: u64 = 3600;
fn settings(key: &[u8]) -> Settings {
    let mut settings = fixtures::settings();
    let forge = settings.forges.get_mut(0).expect("forge");
    forge.kind = Kind::GitHub;
    forge.access = Some(Access {
        credential: Credential::App { id: 7, key: Secret::new(Box::from(key)) },
        user: 9,
        webhook_secret: None,
    });
    settings
}
#[test]
fn a_github_forge_connects_through_listing_minting_and_identity() {
    let mut world = Machine::github(
        SEED,
        fixtures::limits(),
        settings(KEYS.first().expect("public key")),
        [LISTING, PAGE_TWO, MINT, MINT, VIEWER].into_iter().map(Answer::served).collect(),
    );
    world.action(Request::Installations { forge: FORGE, accepted: Box::from([1]) });
    world.run_for(SECOND);
    assert_eq!(world.connected_forges(), &[FORGE]);
    assert_eq!(world.start(FORGE), Some(Start::Ready { cap: 100 }));
    assert_eq!(world.requests().len(), 5);
    assert!(world.script_done());
    assert_eq!(world.git_tokens().len(), 1);
    match world.git_tokens().first().expect("Contents token") {
        Event::GitToken { forge, installation, account, token, expires } => {
            assert_eq!((*forge, *installation, account.id, account.login.as_ref()), (FORGE, 1, 22, b"acme".as_slice()));
            assert!(!token.is_empty());
            assert_eq!(expires.as_secs(), skein_forge_machine_world::github::WALL_BASE.as_secs() + HOUR);
        }
        Event::Installation { .. }
        | Event::Fact { .. }
        | Event::Answered(_)
        | Event::Hint(_)
        | Event::Connected { .. }
        | Event::Stopped { .. }
        | Event::Listening { .. }
        | Event::ListenerLost
        | Event::Closed => panic!("token collection"),
    }
    assert!(world.close());
}
#[test]
fn a_refused_github_key_stops_the_forge() {
    let mut world = Machine::github(
        SEED,
        fixtures::limits(),
        settings(KEYS.get(1).expect("other public key")),
        vec![Answer::served(LISTING)],
    );
    world.action(Request::Installations { forge: FORGE, accepted: Box::from([1]) });
    world.run_for(SECOND);
    assert_eq!(world.stopped(), Some((FORGE, Stopped::KeyRefused)));
    assert!(world.connected_forges().is_empty());
    assert!(world.git_tokens().is_empty());
    assert_eq!(world.requests().len(), 1);
    assert!(world.script_done());
    assert!(world.close());
}
