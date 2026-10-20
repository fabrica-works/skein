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
    let address = skein_fake_peers::Listening::address(&world.peer);
    assert_eq!(Some(address), world.peer.address());
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
    skein_fake_peers::Listening::shutdown(&mut world.peer);
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

#[test]
fn an_endpoint_set_after_binding_delivers_the_next_hook() {
    for (name, host, secret) in fixtures::unusable_endpoints() {
        let mut world = World::delivery(fixtures::SEED);
        world.bind();
        world.forge.borrow_mut().hooks = host.map(|host| fixtures::endpoint(host, secret));
        world.create_issue_and_settle();
        assert_eq!(world.delivery_outcomes(), [&forge::Delivery::NoAddress], "{name}");
        assert_eq!(world.connects, fixtures::NO_POSTS, "{name}");
        assert_eq!(world.received_posts(), fixtures::NO_POSTS, "{name}");
    }
    fixtures::refused_endpoint_updates_preserve_ownership();
    let mut world = World::delivery(fixtures::SEED);
    world.bind_receiver(false);
    world.create_issue_and_settle();
    assert_eq!(world.delivery_outcomes(), [&forge::Delivery::Written]);
    assert_eq!(world.received_posts(), fixtures::ONE_POST);
    assert_eq!(world.connects, fixtures::ONE_POST);
}

#[test]
fn loopback_calls_and_deliveries_leave_frozen_observations() {
    let mut world = World::delivery(fixtures::SEED);
    world.bind_receiver(false);
    let response = world.create_issue_and_settle();
    assert!(response.starts_with(b"HTTP/1.1 201 "));
    assert_eq!(world.domain_calls(), fixtures::WRITE_AND_READBACK);
    assert_eq!(world.received_posts(), fixtures::ONE_POST);
    assert_eq!(fixtures::record_kinds(&world), ["connection", "head", "call", "delivery"]);
    for record in world.peer.observations() {
        if let forge::Observation::Head { call, .. } = record {
            for header in &call.headers {
                if header.name.eq_ignore_ascii_case(b"authorization") {
                    assert_eq!(&*header.value, b"");
                }
            }
        }
    }
    let frozen = world.frozen_trace();
    assert!(fixtures::heads_hide_token(&world));
    world.mutate_owner();
    assert_eq!(world.frozen_trace(), frozen);
}

#[test]
fn late_deliveries_are_written_once() {
    let mut world = World::late_delivery(fixtures::SEED);
    world.bind_receiver(false);
    world.create_issue_and_settle();
    assert_eq!(world.delivery_outcomes(), [&forge::Delivery::Written]);
    assert_eq!(world.received_posts(), fixtures::ONE_POST);
    assert!(world.first_delivery_at() >= fixtures::LATE_MIN);
    assert_eq!(world.connects, fixtures::ONE_POST);
}

#[test]
fn lost_hooks_make_no_delivery_attempt() {
    let mut world = World::lost_delivery(fixtures::SEED);
    world.bind_receiver(false);
    world.create_issue_and_settle();
    assert_eq!(world.delivery_outcomes(), Vec::<&forge::Delivery>::new());
    assert_eq!(world.received_posts(), fixtures::NO_POSTS);
    assert_eq!(world.connects, fixtures::NO_POSTS);
}

#[test]
fn unwritable_deliveries_are_dropped_once() {
    for case in fixtures::unwritable_cases() {
        let mut world = case.world();
        world.inject_hooks(case.hooks);
        world.settle_hooks();
        assert_eq!(world.delivery_outcomes(), case.outcomes, "{}", case.name);
        assert_eq!(world.connects, case.connects, "{}", case.name);
        assert_eq!(world.received_posts(), case.posts, "{}", case.name);
    }
}

#[test]
fn a_seed_replays_names_replies_observations_and_deliveries() {
    let first = fixtures::recorded_delivery(fixtures::SEED);
    let second = fixtures::recorded_delivery(fixtures::SEED);
    assert_eq!(first.peer.observations(), second.peer.observations());
    assert_eq!(first.wire_posts(), second.wire_posts());
    assert_eq!(first.frozen_trace(), second.frozen_trace());
}
