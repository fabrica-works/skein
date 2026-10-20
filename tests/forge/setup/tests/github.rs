use skein_forge_setup::{
    item::{self, Item},
    reconcile::Outcome,
};
use skein_forge_setup_world::{Heard, HostWorld, fixtures};
use skein_lib::{Duration, Token};

#[test]
fn a_forgejo_reconcile_converges_beside_a_configured_github_forge() {
    let settings = fixtures::github_settings();
    let github_address = settings.forges.get(1).expect("GitHub forge").address;
    let mut world = HostWorld::new_with(7, &[], settings, fixtures::host_limits());
    world.run(1);
    world.finish();
    assert_eq!(world.heard(), &[Heard::Done { run: Token::new(1), outcome: Outcome::Nothing, at: Duration::ZERO }]);
    let rendered = format!("{:?}", world.heard());
    for secret in [fixtures::ADMIN, fixtures::API, fixtures::GIT] {
        for bytes in rendered.as_bytes().windows(secret.len()) {
            assert_ne!(bytes, secret, "credentials stay out of Debug");
        }
    }
    assert_eq!(world.file(Item::User).expect("User written").expose(), &2u64.to_le_bytes());
    for key in [Item::ApiToken, Item::GitToken] {
        assert!(item::token(world.file(key).expect("token written").expose(), 64).is_some());
    }
    assert!(!world.connection_addresses().is_empty());
    assert!(!world.connection_addresses().contains(&github_address));
    let mut plain = HostWorld::new(7, &[]);
    plain.run(1);
    plain.finish();
    for key in [Item::User, Item::AdminToken, Item::ApiToken, Item::GitToken, Item::WebhookSecret] {
        assert_eq!(world.file(key), plain.file(key));
    }
}
