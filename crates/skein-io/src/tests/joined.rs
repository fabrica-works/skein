//! Named admission and connector ownership (io.md, 3.5).
use super::{Rig, limits, name, owner};
use crate::{Error, Event, Limits, Name, Request};
use alloc::boxed::Box;
use skein_lib::Token;

fn listen(rig: &mut Rig, who: u64, bytes: &[u8]) -> Token {
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

fn pair() -> (Rig, Token, Token, Token) {
    let mut rig = Rig::new(Limits { sockets: 5, ..limits() });
    let listener = listen(&mut rig, 1, b"engine");
    rig.down(Request::Connect { owner: owner(2), addr: name(b"engine") }).nothing();
    let connecting = rig.next();
    let connector = match connecting.events.as_slice() {
        [Event::Connecting { socket, .. }] => *socket,
        other => panic!("Connecting: {other:?}"),
    };
    let accepted = rig.next();
    let socket = match accepted.events.as_slice() {
        [Event::Accepted { socket, .. }] => *socket,
        other => panic!("Accepted: {other:?}"),
    };
    assert_eq!(rig.next().events, [Event::Connected { owner: owner(2) }]);
    (rig, listener, connector, socket)
}
fn bound() -> (Rig, Token, Token) {
    let (mut rig, _, connector, socket) = pair();
    rig.down(Request::Bind { socket, owner: owner(3) }).nothing();
    rig.next().nothing();
    (rig, connector, socket)
}
fn stream(rig: &mut Rig, socket: Token, down: skein_lib::stream::Down) {
    rig.down(Request::Stream { stream: socket, down }).nothing();
}
fn send(rig: &mut Rig, socket: Token, data: &[u8]) {
    use skein_lib::stream::{Down, Read};
    stream(rig, socket, Down::Demand { read: Read::Nothing, room: u32::try_from(data.len()).unwrap() });
    assert!(rig.next().events.contains(&Event::Stream { owner: owner(2), up: skein_lib::stream::Up::Room }));
    stream(rig, socket, Down::Send(Box::from(data)));
}

#[test]
fn an_armed_listener_joins_the_first_waiting_connector() {
    let mut rig = Rig::new(Limits { sockets: 5, ..limits() });
    let _listener = listen(&mut rig, 1, b"engine");
    for who in [2, 4] {
        rig.down(Request::Connect { owner: owner(who), addr: name(b"engine") }).nothing();
    }
    assert_eq!(rig.next().events.len(), 2);
    assert_eq!(rig.next().events.len(), 1);
    assert_eq!(rig.next().events, [Event::Connected { owner: owner(2) }]);
}

#[test]
fn the_accepted_end_is_announced_with_the_empty_name() {
    let mut rig = Rig::new(Limits { sockets: 3, ..limits() });
    let _listener = listen(&mut rig, 1, b"engine");
    rig.down(Request::Connect { owner: owner(2), addr: name(b"engine") }).nothing();
    let _connecting = rig.next();
    let out = rig.next();
    match out.events.as_slice() {
        [Event::Accepted { peer, .. }] => {
            assert_eq!(*peer, crate::Addr::InProcess(Name::EMPTY));
        }
        other => panic!("accepted: {other:?}"),
    }
}

#[test]
fn a_rejected_end_fails_its_connector_with_reset() {
    let (mut rig, _, _, socket) = pair();
    rig.down(Request::Reject { socket }).nothing();
    rig.next().nothing();
    assert_eq!(
        rig.next().events,
        [Event::Stream { owner: owner(2), up: skein_lib::stream::Up::Failed(skein_lib::stream::Fault::Reset) }]
    );
}

#[test]
fn a_listener_without_room_leaves_its_connectors_waiting() {
    let (mut rig, _, _, _) = pair();
    rig.down(Request::Connect { owner: owner(4), addr: name(b"engine") }).nothing();
    let _connecting = rig.next();
    rig.next().nothing();
}

#[test]
fn bytes_move_into_the_peer_only_as_far_as_its_intake_has_room() {
    use skein_lib::stream::{Down, Read, Up};
    let (mut rig, connector, socket) = bound();
    send(&mut rig, connector, b"abcdefgh");
    rig.next().nothing();
    rig.next().nothing();
    send(&mut rig, connector, b"ijkl");
    rig.next().nothing();
    stream(&mut rig, socket, Down::Demand { read: Read::Fill(8), room: 0 });
    assert_eq!(rig.next().events, [Event::Stream { owner: owner(3), up: Up::Bytes(Box::from(&b"abcdefgh"[..])) }]);
    rig.next().nothing();
    stream(&mut rig, socket, Down::Demand { read: Read::Fill(4), room: 0 });
    assert_eq!(rig.next().events, [Event::Stream { owner: owner(3), up: Up::Bytes(Box::from(&b"ijkl"[..])) }]);
}

#[test]
fn a_finish_reaches_the_peer_as_end_after_the_last_byte() {
    use skein_lib::stream::{Down, Read, Up};
    let (mut rig, connector, socket) = bound();
    send(&mut rig, connector, b"hello");
    stream(&mut rig, connector, Down::Finish);
    rig.next().nothing();
    stream(&mut rig, socket, Down::Demand { read: Read::Fill(5), room: 0 });
    assert_eq!(
        rig.next().events,
        [
            Event::Stream { owner: owner(3), up: Up::Bytes(Box::from(&b"hello"[..])) },
            Event::Stream { owner: owner(3), up: Up::End }
        ]
    );
}

#[test]
fn an_abort_fails_the_peer_with_reset_at_once() {
    let (mut rig, connector, _) = bound();
    rig.down(Request::Abort { entity: connector }).nothing();
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(2) }]);
    assert_eq!(
        rig.next().events,
        [Event::Stream { owner: owner(3), up: skein_lib::stream::Up::Failed(skein_lib::stream::Fault::Reset) }]
    );
}

#[test]
fn a_close_moves_what_is_queued_before_unjoining() {
    use skein_lib::stream::{Down, Read, Up};
    let (mut rig, connector, socket) = bound();
    send(&mut rig, connector, b"hello");
    rig.down(Request::Close { entity: connector }).nothing();
    rig.next().nothing();
    stream(&mut rig, socket, Down::Demand { read: Read::Fill(5), room: 0 });
    assert_eq!(
        rig.next().events,
        [
            Event::Stream { owner: owner(3), up: Up::Bytes(Box::from(&b"hello"[..])) },
            Event::Stream { owner: owner(3), up: Up::End }
        ]
    );
    stream(&mut rig, socket, Down::Finish);
    rig.next().nothing();
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(2) }]);
}

#[test]
fn a_closing_end_discards_what_its_peer_moves_until_end() {
    use skein_lib::stream::Down;
    let (mut rig, connector, socket) = bound();
    rig.down(Request::Close { entity: socket }).nothing();
    let _ended = rig.next();
    send(&mut rig, connector, b"abcdefgh");
    rig.next().nothing();
    stream(&mut rig, connector, Down::Finish);
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(3) }]);
}

#[test]
fn a_close_with_nothing_to_flush_is_closed_next_pass() {
    use skein_lib::stream::Down;
    let (mut rig, connector, socket) = bound();
    stream(&mut rig, connector, Down::Finish);
    rig.next().nothing();
    let _ended = rig.next();
    rig.down(Request::Close { entity: socket }).nothing();
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(3) }]);
}

#[test]
fn a_joined_end_submits_nothing_at_any_entry_point() {
    let (mut rig, connector, socket) = bound();
    send(&mut rig, connector, b"hello");
    assert!(rig.next().subs.is_empty());
    rig.down(Request::Abort { entity: connector }).nothing();
    assert!(rig.next().subs.is_empty());
    assert!(rig.next().subs.is_empty());
    rig.down(Request::Abort { entity: socket }).nothing();
    assert!(rig.next().subs.is_empty());
}

#[test]
fn an_abort_before_bind_fails_the_bound_end() {
    let (mut rig, _, connector, socket) = pair();
    rig.down(Request::Abort { entity: connector }).nothing();
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(2) }]);
    rig.down(Request::Bind { socket, owner: owner(3) }).nothing();
    assert_eq!(
        rig.next().events,
        [Event::Stream { owner: owner(3), up: skein_lib::stream::Up::Failed(skein_lib::stream::Fault::Reset) }]
    );
}

fn output(rig: &mut Rig, socket: Token, down: skein_lib::stream::OutputDown) {
    rig.down(Request::Output { stream: socket, down }).nothing();
}
fn terminal(who: u64, right: u64, outcome: skein_lib::stream::OutputOutcome) -> Event {
    Event::Output { owner: owner(who), up: skein_lib::stream::OutputUp::Settled { right: Token::new(right), outcome } }
}
#[test]
fn the_close_deadline_ends_a_close_the_peer_never_ends() {
    use skein_lib::{Duration, Time};
    let (mut rig, connector, socket) = bound();
    rig.down(Request::Close { entity: socket }).nothing();
    let _ended = rig.next();
    assert_eq!(rig.io.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(1))));
    assert_eq!(rig.at(Time::from_nanos(1_000_000_000)).events, [Event::Closed { owner: owner(3) }]);
    assert_eq!(rig.next().events, [Event::Stream { owner: owner(2), up: skein_lib::stream::Up::End }]);
    rig.down(Request::Close { entity: connector }).nothing();
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(2) }]);
}
#[test]
fn an_end_whose_peer_closed_fails_reset_at_its_next_move() {
    use skein_lib::stream::{Fault, Up};
    let (mut rig, connector, socket) = bound();
    rig.down(Request::Close { entity: socket }).nothing();
    let _ended = rig.next();
    assert_eq!(rig.at(skein_lib::Time::from_nanos(1_000_000_000)).events, [Event::Closed { owner: owner(3) }]);
    send(&mut rig, connector, b"late");
    assert_eq!(rig.next().events, [Event::Stream { owner: owner(2), up: Up::Failed(Fault::Reset) }]);
}
#[test]
fn a_peer_past_its_close_deadline_fails_the_end_with_reset() {
    use skein_lib::stream::{Fault, Up};
    let (mut rig, connector, socket) = bound();
    send(&mut rig, connector, b"queued");
    rig.down(Request::Close { entity: socket }).nothing();
    assert_eq!(rig.at(skein_lib::Time::from_nanos(1_000_000_000)).events, [Event::Closed { owner: owner(3) }]);
    assert_eq!(rig.next().events, [Event::Stream { owner: owner(2), up: Up::Failed(Fault::Reset) }]);
}

#[test]
fn an_output_reservation_on_a_joined_end_is_granted_then_spent() {
    use skein_lib::stream::{Down, OutputDown, OutputOutcome, Read, Up};
    let (mut rig, connector, socket) = bound();
    stream(&mut rig, connector, Down::Demand { read: Read::Fill(2), room: 0 });
    output(&mut rig, connector, OutputDown::Room { right: Token::new(10), bytes: 2 });
    assert_eq!(rig.next().events, [terminal(2, 10, OutputOutcome::Granted)]);
    output(&mut rig, connector, OutputDown::Send { right: Token::new(10), bytes: Box::from(&b"hi"[..]) });
    rig.next().nothing();
    stream(&mut rig, socket, Down::Demand { read: Read::Fill(2), room: 0 });
    assert_eq!(rig.next().events, [Event::Stream { owner: owner(3), up: Up::Bytes(Box::from(&b"hi"[..])) }]);
    output(&mut rig, socket, OutputDown::Room { right: Token::new(11), bytes: 2 });
    assert_eq!(rig.next().events, [terminal(3, 11, OutputOutcome::Granted)]);
    output(&mut rig, socket, OutputDown::Send { right: Token::new(11), bytes: Box::from(&b"ok"[..]) });
    rig.next().nothing();
    assert_eq!(rig.next().events, [Event::Stream { owner: owner(2), up: Up::Bytes(Box::from(&b"ok"[..])) }]);
}
#[test]
fn a_close_cancels_a_pending_output_reservation_before_closed() {
    use skein_lib::stream::{OutputDown, OutputOutcome};
    let (mut rig, connector, socket) = bound();
    output(&mut rig, connector, OutputDown::Room { right: Token::new(10), bytes: 8 });
    rig.down(Request::Close { entity: connector }).nothing();
    let out = rig.next();
    assert_eq!(out.events.first(), Some(&terminal(2, 10, OutputOutcome::Cancelled)));
    rig.down(Request::Close { entity: socket }).nothing();
    let out = rig.next();
    assert!(out.events.contains(&Event::Closed { owner: owner(3) }));
    assert_eq!(rig.next().events, [Event::Closed { owner: owner(2) }]);
}
#[test]
fn an_abort_settles_both_pending_outputs_from_their_own_ends() {
    use skein_lib::stream::{Fault, OutputDown, OutputOutcome, Up};
    let (mut rig, connector, socket) = bound();
    output(&mut rig, connector, OutputDown::Room { right: Token::new(10), bytes: 8 });
    output(&mut rig, socket, OutputDown::Room { right: Token::new(11), bytes: 8 });
    rig.down(Request::Abort { entity: connector }).nothing();
    assert_eq!(
        rig.next().events,
        [
            terminal(2, 10, OutputOutcome::Cancelled),
            Event::Closed { owner: owner(2) },
            terminal(3, 11, OutputOutcome::Failed(Fault::Reset)),
            Event::Stream { owner: owner(3), up: Up::Failed(Fault::Reset) }
        ]
    );
}

#[test]
fn a_joined_deadline_settles_both_pending_outputs_from_their_own_ends() {
    use skein_lib::stream::{Fault, OutputDown, OutputOutcome, Up};
    let (mut rig, connector, socket) = bound();
    send(&mut rig, connector, b"queued");
    output(&mut rig, connector, OutputDown::Room { right: Token::new(10), bytes: 8 });
    output(&mut rig, socket, OutputDown::Room { right: Token::new(11), bytes: 8 });
    rig.down(Request::Close { entity: socket }).nothing();
    assert_eq!(
        rig.at(skein_lib::Time::from_nanos(1_000_000_000)).events,
        [terminal(3, 11, OutputOutcome::Cancelled), Event::Closed { owner: owner(3) }]
    );
    assert_eq!(
        rig.next().events,
        [
            terminal(2, 10, OutputOutcome::Failed(Fault::Reset)),
            Event::Stream { owner: owner(2), up: Up::Failed(Fault::Reset) }
        ]
    );
}

#[test]
fn a_joined_end_meets_a_read_of_what_has_arrived_with_what_its_peer_moved() {
    use skein_lib::stream::{Down, Read, Up};
    let (mut rig, connector, socket) = bound();
    send(&mut rig, connector, b"hello");
    rig.next().nothing();
    rig.next().nothing();
    stream(&mut rig, socket, Down::Demand { read: Read::Arrived { max: 64 }, room: 0 });
    assert_eq!(rig.next().events, [Event::Stream { owner: owner(3), up: Up::Bytes(Box::from(&b"hello"[..])) }]);
}
