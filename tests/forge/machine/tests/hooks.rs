use skein_forge::api::{Change, Repository};
use skein_forge_connection::Request;
use skein_forge_machine_world::{
    fixtures,
    hooks::{self as deliveries, Listener},
};
use skein_lib::{Duration, bytes};
const SECOND: Duration = Duration::from_secs(1);
fn world() -> Listener {
    Listener::new(7, fixtures::hook_limits(), fixtures::hook_settings())
}
#[test]
fn a_signed_push_is_answered_204_before_its_hint_goes_up() {
    let mut world = world();
    let peer = world.accept();
    world.send(peer, &deliveries::push(b"main", 7));
    world.run_for(SECOND);
    assert_eq!(world.status(peer), Some(204));
    let [hinted] = world.hints() else { panic!("one hint") };
    assert_eq!(hinted.hint.repository, Repository { forge: 1, repository: 2 });
    assert_eq!(
        hinted.hint.change,
        Change::BranchMoved {
            branch: bytes::copy_of(b"main"),
            head: skein_fake_forge_protocol::routes::object(7).bytes,
            actor: 2
        }
    );
    assert!(world.hinted_after_answer());
}
#[test]
fn a_wrong_signature_is_answered_403_and_hints_nothing() {
    let mut world = world();
    let peer = world.accept();
    let mut post = deliveries::push(b"main", 7);
    post.body[0] = b' ';
    world.send(peer, &post);
    assert_eq!(world.status(peer), Some(403));
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_body_past_its_cap_is_answered_413_and_hints_nothing() {
    let mut world = world();
    let peer = world.accept();
    world.write(peer, b"POST /hooks/1 HTTP/1.1\r\nHost: loopback\r\nContent-Length: 1048577\r\n\r\n");
    assert_eq!(world.status(peer), Some(413));
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_path_without_a_forge_is_answered_404() {
    let mut world = world();
    let peer = world.accept();
    world.send(peer, &deliveries::signed(b"/hooks/9", b"ping", fixtures::PING));
    assert_eq!(world.status(peer), Some(404));
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_path_without_a_secret_is_answered_404() {
    let mut settings = fixtures::hook_settings();
    settings.forges.get_mut(0).expect("forge").access.as_mut().expect("fixture access").webhook_secret = None;
    let mut world = Listener::new(7, fixtures::hook_limits(), settings);
    let peer = world.accept();
    world.send(peer, &deliveries::signed(b"/hooks/1", b"ping", fixtures::PING));
    assert_eq!(world.status(peer), Some(404));
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_get_to_a_forges_path_is_answered_405() {
    let mut world = world();
    let peer = world.accept();
    world.write(peer, b"GET /hooks/1 HTTP/1.1\r\nHost: loopback\r\nContent-Length: 999\r\n\r\n");
    assert_eq!(world.status(peer), Some(405));
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_missing_signature_is_answered_403_without_reading_a_body() {
    let mut world = world();
    let peer = world.accept();
    world.write(peer, b"POST /hooks/1 HTTP/1.1\r\nHost: loopback\r\nContent-Length: 999\r\n\r\n");
    assert_eq!(world.status(peer), Some(403));
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_verified_body_the_decoder_calls_malformed_is_answered_400() {
    let mut world = world();
    let peer = world.accept();
    world.send(peer, &deliveries::signed(b"/hooks/1", b"push", b"{}"));
    assert_eq!(world.status(peer), Some(400));
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_verified_event_of_no_row_is_answered_204_and_hints_nothing() {
    let mut world = world();
    let peer = world.accept();
    world.send(peer, &deliveries::signed(b"/hooks/1", b"ping", fixtures::PING));
    assert_eq!(world.status(peer), Some(204));
    assert!(world.hints().is_empty());
    assert!(!world.peer(peer).closed);
}
#[test]
fn a_hint_the_root_refused_is_held_and_offered_again() {
    let mut world = world();
    let peer = world.accept();
    world.send(peer, &deliveries::push(b"main", 7));
    let expected = world.hints()[0].clone();
    world.refuse();
    world.run_for(SECOND);
    assert!(world.hints().is_empty());
    world.request(Request::Offer);
    assert_eq!(world.hints(), &[expected]);
}
#[test]
fn a_connection_past_hooks_waits_until_one_closes() {
    let mut world = world();
    let first = world.accept();
    for _ in 1..fixtures::hook_limits().hooks {
        world.accept();
    }
    let waiting = world.accept();
    world.send(waiting, &deliveries::push(b"main", 7));
    assert_eq!(world.peer(waiting).owner, None);
    assert_eq!(world.status(waiting), None);
    world.close(first);
    assert_eq!(world.status(waiting), Some(204));
    assert_eq!(world.hints().len(), 1);
}
#[test]
fn a_request_unfinished_at_its_deadline_is_closed_with_no_answer() {
    let mut world = world();
    let peer = world.accept();
    world.write(peer, b"POST /hooks/1 HTTP/1.1\r\nHost:");
    world.run_for(fixtures::hook_limits().hook_deadline);
    assert_eq!(world.status(peer), None);
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_failed_listener_is_told_to_the_root_as_lost() {
    let mut world = world();
    world.lose();
    assert_eq!(world.lost, 1);
}
#[test]
fn a_recorded_forgejo_push_signed_by_the_world_is_hinted() {
    let mut world = world();
    let peer = world.accept();
    let request = include_bytes!("../../conformance/deliveries/push/request");
    let offset = bytes::find(request, b"\r\n\r\n").expect("recorded head") + 4;
    world.send(peer, &deliveries::signed(b"/hooks/1", b"push", &request[offset..]));
    assert_eq!(world.status(peer), Some(204));
    assert_eq!(world.hints().len(), 1);
}
#[test]
fn the_secret_appears_in_no_event_or_debug_text() {
    let mut world = world();
    let peer = world.accept();
    world.send(peer, &deliveries::push(b"main", 7));
    let debug = format!("{} {:?}", world.debug(), world.hints());
    let secret = core::str::from_utf8(fixtures::HOOK_SECRET).expect("secret");
    let numbers = format!("{:?}", fixtures::HOOK_SECRET);
    assert!(!debug.contains(secret));
    assert!(!debug.contains(&numbers[1..numbers.len() - 1]));
}
#[test]
fn two_deliveries_on_one_connection_are_both_answered_and_hinted() {
    let mut world = world();
    let peer = world.accept();
    world.send(peer, &deliveries::push(b"main", 7));
    world.send(peer, &deliveries::push(b"other", 8));
    assert_eq!(world.statuses(peer), [204, 204]);
    assert_eq!(world.hints().len(), 2);
    assert!(!world.peer(peer).closed);
}
#[test]
fn a_malformed_head_is_answered_4xx_and_closed() {
    let mut world = world();
    let peer = world.accept();
    world.write(peer, b"POST /hooks/1 HTTP/1.1\r\nHost: loopback\r\nBroken field\r\n\r\n");
    assert_eq!(world.status(peer), Some(400));
    assert!(world.hints().is_empty());
    assert!(world.peer(peer).closed);
}
#[test]
fn a_domain_push_reaches_the_component_as_a_signed_delivery_and_a_hint() {
    let mut world = skein_forge_machine_world::joint::Joint::hooked(7);
    world.run_for(SECOND);
    world.push(b"main", b"changed");
    world.run_for(SECOND);
    let pushes: Vec<_> =
        world.hints.iter().filter(|hint| matches!(hint.hint.change, Change::BranchMoved { .. })).collect();
    assert_eq!(pushes.len(), 1);
    assert_eq!(pushes[0].hint.repository, Repository { forge: 1, repository: 2 });
    let Change::BranchMoved { branch, actor, .. } = &pushes[0].hint.change else { panic!("push") };
    assert_eq!((branch.as_ref(), *actor), (b"main".as_slice(), 2));
}
#[test]
fn a_held_hint_stops_the_next_body_until_it_is_offered() {
    let mut world = world();
    let peer = world.accept();
    world.send(peer, &deliveries::push(b"main", 7));
    world.refuse();
    world.send(peer, &deliveries::push(b"other", 8));
    assert_eq!(world.statuses(peer), [204]);
    assert!(world.hints().is_empty());
    world.request(Request::Offer);
    assert_eq!(world.statuses(peer), [204, 204]);
    assert_eq!(world.hints().len(), 2);
}

#[derive(Clone, Copy, Debug)]
enum HookPhase {
    Head,
    Body,
    Answering,
    Closing,
}

#[test]
fn deadlines_end_each_hook_phase() {
    for (phase, seconds) in
        [(HookPhase::Head, 10), (HookPhase::Body, 10), (HookPhase::Answering, 10), (HookPhase::Closing, 1)]
    {
        assert_eq!(drive_hook_deadline(phase, seconds), [0, 0, 1, 0], "{phase:?}");
    }
}

fn drive_hook_deadline(phase: HookPhase, seconds: u64) -> [u64; 4] {
    use skein_lib::Time;
    let mut world = world();
    let socket = world.accept();
    let post = deliveries::push(b"main", 7);
    match phase {
        HookPhase::Head => world.write(socket, &post.head[..post.head.len() - 1]),
        HookPhase::Body => {
            world.write(socket, &post.head);
            world.write(socket, &post.body[..post.body.len() - 1]);
        }
        HookPhase::Answering => {
            world.send(socket, &post);
            assert_eq!((world.counts()[0], world.hints().len()), (1, 1));
            assert!(world.hinted_after_answer());
            world.refuse();
            assert_eq!(world.hints().len(), 0);
        }
        HookPhase::Closing => {
            world.hold_settlement();
            world.fault(socket, true);
        }
    }
    let before = world.counts();
    let queued = u64::from(matches!(phase, HookPhase::Answering));
    assert_eq!(&before[..2], &[queued, queued], "{phase:?} before expiry");
    assert_eq!(world.hints().len(), 0, "{phase:?} before expiry");
    assert_eq!(before[2], 0, "{phase:?} before expiry");
    world.advance_to(Time::from_nanos(seconds * 1_000_000_000 - 1));
    assert_eq!(world.counts()[2] - before[2], 0, "{phase:?} before expiry");
    world.advance_to(Time::from_nanos(seconds * 1_000_000_000));
    let after = world.counts();
    [after[0] - before[0], after[1] - before[1], after[2] - before[2], after[3]]
}

#[test]
fn a_secret_handed_by_connect_verifies_the_next_delivery() {
    let mut settings = fixtures::hook_settings();
    settings.forges.get_mut(0).expect("forge").access.as_mut().expect("fixture access").webhook_secret = None;
    let mut world = Listener::new(7, fixtures::hook_limits(), settings);
    let peer = world.accept();
    world.send(peer, &deliveries::push(b"main", 7));
    assert_eq!(world.status(peer), Some(404));
    world.request(Request::Connect {
        forge: fixtures::FORGE,
        access: fixtures::access(fixtures::TOKEN, fixtures::WRITER, Some(fixtures::HOOK_SECRET)),
    });
    let peer = world.accept();
    world.send(peer, &deliveries::push(b"main", 7));
    world.run_for(SECOND);
    assert_eq!(world.status(peer), Some(204));
    assert_eq!(world.hints().len(), 1);
    let debug = format!("{} {:?}", world.debug(), world.hints());
    for secret in [fixtures::TOKEN, fixtures::HOOK_SECRET] {
        assert!(!debug.as_bytes().windows(secret.len()).any(|window| window == secret));
    }
}
