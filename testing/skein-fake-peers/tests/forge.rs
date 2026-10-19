//! Shared forge names, loopback admission and real byte settlement on Sim.
use skein_fake_peers::{Error, forge};
use skein_lib::Time;
use skein_world::Host;

#[path = "forge/fixtures.rs"]
mod fixtures;
mod support;
use fixtures::{ONE_REPLY, World};

#[test]
fn forge_peer_refuses_unusable_configuration_before_effects() {
    for case in fixtures::refusals() {
        assert_eq!(
            forge::Peer::new(case.address, case.limits, case.config, case.protocol, case.shared, fixtures::SEED).err(),
            Some(Error::Limits),
            "{}",
            case.name
        );
    }
}

#[test]
fn forge_peer_publishes_its_bound_loopback_address() {
    let mut world = World::new();
    assert_eq!(world.peer.address(), None);
    world.bind();
    let address = world.peer.address().expect("listening address");
    assert!(address.ip().is_loopback());
    assert_ne!(address.port(), 0);
    assert!(world.peer.worst_case() > 0);
    world.settle();
}

#[test]
fn a_second_fake_shares_names_replies_and_time() {
    let mut world = World::new();
    world.bind();
    let (commit, git_replies, last_now) = world.git_push();
    assert_eq!(git_replies, ONE_REPLY);
    assert_eq!(commit, fixtures::PUSHED_COMMIT);
    assert_eq!(last_now, world.forge.borrow().now);
    assert_ne!(last_now, Time::ZERO);
    let fd = world.open();
    world.send(fd, &fixtures::request(b"/api/v1/repos/org/repo/branches/work"));
    let response = world.response(fd);
    assert!(response.starts_with(b"HTTP/1.1 200 "));
    assert!(response.ends_with(fixtures::WORK_BRANCH_BODY));
    let branch = world
        .peer
        .observations()
        .iter()
        .find_map(|record| {
            if let forge::Observation::Call {
                result: Ok(skein_fake_forge_domain::api::Answer::Commit(commit)), ..
            } = record
            {
                Some(*commit)
            } else {
                None
            }
        })
        .expect("HTTP branch call's frozen terminal");
    assert_eq!(branch, commit);
    assert_eq!(world.replies, ONE_REPLY);
    assert!(world.forge.borrow().pending.is_empty());
    world.close(fd);
    world.settle();
}

#[test]
fn shutdown_keeps_a_mid_request_client_until_its_terminal() {
    let mut world = World::new();
    let fd = world.open();
    let request = fixtures::request(b"/api/v1/repos/org/repo/branches/main");
    let split = b"GET /api/v1/repos/org/repo/branches/main HTTP/1.1\r\n".len();
    world.send(fd, &request[..split]);
    world.peer.shutdown();
    world.pass();
    assert!(!world.peer.is_empty());
    world.send(fd, &request[split..]);
    let response = world.response(fd);
    assert!(response.starts_with(b"HTTP/1.1 200 "));
    assert_eq!(world.replies, ONE_REPLY);
    assert_eq!(
        world.peer.observations().iter().filter(|record| matches!(record, forge::Observation::Call { .. })).count(),
        ONE_REPLY
    );
    world.close(fd);
    world.settle();
    assert!(world.peer.is_empty());
}
