//! Named admission and connector ownership (io.md, 3.5).
use super::{Rig, limits, name, owner};
use crate::{Error, Event, Limits, Name, Request};

fn listen(rig: &mut Rig, who: u64, bytes: &[u8]) -> skein_lib::Token {
    rig.down(Request::Listen { owner: owner(who), addr: name(bytes) }).nothing();
    let out = rig.next();
    assert!(out.subs.is_empty(), "joined entities submit nothing");
    match out.events.as_slice() {
        [Event::Listening { listener, .. }] => *listener,
        other => panic!("listener is told: {other:?}"),
    }
}

#[test]
fn a_listen_on_a_name_is_told_listening_and_submits_nothing() {
    let mut rig = Rig::new(limits());
    rig.down(Request::Listen { owner: owner(1), addr: name(b"engine") }).nothing();
    let out = rig.next();
    assert!(out.subs.is_empty(), "a joined listener submits nothing: {out:?}");
    match out.events.as_slice() {
        [Event::Listening { owner: told, addr, .. }] => {
            assert_eq!((*told, *addr), (owner(1), name(b"engine")), "told at the requested name");
        }
        other => panic!("a listener on a name is told Listening: {other:?}"),
    }
}

#[test]
fn a_second_listen_on_a_taken_name_fails_with_other() {
    let mut rig = Rig::new(limits());
    let _listener = listen(&mut rig, 1, b"engine");
    rig.down(Request::Listen { owner: owner(2), addr: name(b"engine") }).nothing();
    assert_eq!(
        rig.next().events,
        [Event::Failed { owner: owner(2), error: Error::Other }, Event::Closed { owner: owner(2) }],
        "failed then closed for its owner"
    );
}

#[test]
fn a_listen_on_the_empty_name_fails_with_other() {
    let mut rig = Rig::new(limits());
    rig.down(Request::Listen { owner: owner(1), addr: name(b"") }).nothing();
    assert_eq!(
        rig.next().events,
        [Event::Failed { owner: owner(1), error: Error::Other }, Event::Closed { owner: owner(1) }],
        "failed then closed for its owner"
    );
    rig.empty();
}

#[test]
fn a_listen_past_the_names_limit_is_refused_busy() {
    let mut rig = Rig::new(Limits { names: 1, ..limits() });
    let _listener = listen(&mut rig, 1, b"engine");
    rig.down(Request::Listen { owner: owner(2), addr: name(b"worker") }).nothing();
    assert_eq!(
        rig.next().events,
        [Event::Failed { owner: owner(2), error: Error::Busy }, Event::Closed { owner: owner(2) }],
        "failed then closed for its owner"
    );
}

#[test]
fn a_closed_listener_frees_its_name_for_the_next_listen() {
    let mut rig = Rig::new(Limits { names: 1, ..limits() });
    let listener = listen(&mut rig, 1, b"engine");
    rig.down(Request::Close { entity: listener }).nothing();
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(1) }], "the listener closes next pass");
    rig.empty();
    let _listener = listen(&mut rig, 2, b"engine");
}

#[test]
fn a_connect_to_an_unbound_name_is_refused_then_closed() {
    let mut rig = Rig::new(limits());
    rig.down(Request::Connect { owner: owner(1), addr: name(b"lost") }).nothing();
    let out = rig.next();
    assert!(out.subs.is_empty(), "joined entities submit nothing");
    match out.events.as_slice() {
        [
            Event::Connecting { owner: told, .. },
            Event::Failed { owner: failed, error: Error::Refused },
            Event::Closed { owner: closed },
        ] => assert_eq!(
            (*told, *failed, *closed),
            (owner(1), owner(1), owner(1)),
            "one connector is refused then closed"
        ),
        other => panic!("Connecting, Refused and Closed in order: {other:?}"),
    }
    rig.empty();
}

#[test]
fn a_connector_waits_on_a_bound_name_until_its_abort() {
    let mut rig = Rig::new(limits());
    let _listener = listen(&mut rig, 1, b"engine");
    rig.down(Request::Connect { owner: owner(2), addr: name(b"engine") }).nothing();
    let out = rig.next();
    let socket = match out.events.as_slice() {
        [Event::Connecting { socket, .. }] => *socket,
        other => panic!("waiting connector is told its token: {other:?}"),
    };
    rig.next().nothing();
    rig.down(Request::Abort { entity: socket }).nothing();
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(2) }], "an aborted connector closes next pass");
}

#[test]
fn a_closing_listener_refuses_the_connectors_waiting_on_it() {
    let mut rig = Rig::new(limits());
    let listener = listen(&mut rig, 1, b"engine");
    rig.down(Request::Connect { owner: owner(2), addr: name(b"engine") }).nothing();
    let _connecting = rig.next();
    rig.down(Request::Close { entity: listener }).nothing();
    rig.next().nothing();
    let out = rig.next();
    assert_eq!(
        out.events,
        [
            Event::Closed { owner: owner(1) },
            Event::Failed { owner: owner(2), error: Error::Refused },
            Event::Closed { owner: owner(2) }
        ],
        "the listener closes and the waiting connector is refused then closed"
    );
    rig.empty();
}

#[test]
fn names_are_inline_and_preserve_all_bytes_up_to_the_bound() {
    for bytes in [&b""[..], &b"a\0b"[..], &[7; 32][..]] {
        assert_eq!(Name::new(bytes).unwrap().as_bytes(), bytes, "inline names preserve their bytes");
    }
}

#[test]
fn a_name_past_the_bound_is_refused() {
    assert!(Name::new(&[7; 33]).is_none(), "names past MAX are refused");
}

#[test]
fn names_compare_as_bytes_including_trailing_zeroes() {
    assert!(Name::new(b"a").unwrap() < Name::new(b"a\0").unwrap(), "a byte prefix sorts before its extension");
}
