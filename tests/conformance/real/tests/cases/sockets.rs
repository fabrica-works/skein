use super::BACKEND;
use crate::on;
use skein_conformance::Check;
use skein_conformance::{
    Cancelling, Pairing, Race, address_in_use, backpressure, cancel_accept, cancel_accept_racing_a_connect,
    cancel_connect, cancel_connect_established_while_away, cancel_recv, cancel_recv_racing_bytes, closed_before_accept,
    full_accept_queue, graceful_close, ipv6_only, lifecycle, listener_close_resets_waiting, refused,
    reset_after_end_of_stream, send_after_peer_closed, unread_close_meets_recv, unread_close_meets_send, wrong_state,
};
use skein_conformance_real::Real;
use skein_io::kernel::Family;

#[test]
fn a_connection_lives_and_ends_over_ipv4() {
    let seen = lifecycle(&mut Real::new(BACKEND), Family::Ipv4);
    #[cfg(target_os = "linux")]
    seen.check();
    #[cfg(target_os = "macos")]
    seen.check_darwin();
}

#[test]
fn a_connection_lives_and_ends_over_ipv6() {
    let seen = lifecycle(&mut Real::new(BACKEND), Family::Ipv6);
    #[cfg(target_os = "linux")]
    seen.check();
    #[cfg(target_os = "macos")]
    seen.check_darwin();
}

#[test]
fn a_peer_closes_gracefully() {
    on(BACKEND, graceful_close);
}

#[test]
fn a_send_after_the_peer_closed() {
    let seen = send_after_peer_closed(&mut Real::new(BACKEND));
    #[cfg(target_os = "linux")]
    seen.check();
    #[cfg(target_os = "macos")]
    seen.check_darwin();
}

#[test]
fn a_connect_where_nothing_listens() {
    on(BACKEND, refused);
}

#[test]
fn where_an_address_is_in_use() {
    let seen = address_in_use(&mut Real::new(BACKEND));
    #[cfg(target_os = "linux")]
    seen.check();
    #[cfg(target_os = "macos")]
    seen.check_darwin();
}

#[test]
fn ipv6_sockets_are_ipv6_only() {
    on(BACKEND, ipv6_only);
}

#[test]
fn records_wrong_for_the_socket_state() {
    on(BACKEND, wrong_state);
}

#[test]
fn a_full_accept_queue() {
    on(BACKEND, full_accept_queue);
}

#[test]
fn a_close_with_bytes_unread_meets_a_recv() {
    let seen = unread_close_meets_recv(&mut Real::new(BACKEND));
    #[cfg(target_os = "linux")]
    seen.check();
    #[cfg(target_os = "macos")]
    seen.check_darwin();
}

#[test]
fn a_close_with_bytes_unread_meets_a_send() {
    let seen = unread_close_meets_send(&mut Real::new(BACKEND));
    #[cfg(target_os = "linux")]
    seen.check();
    #[cfg(target_os = "macos")]
    seen.check_darwin();
}

#[test]
fn a_reset_after_the_end_of_stream() {
    let seen = reset_after_end_of_stream(&mut Real::new(BACKEND));
    #[cfg(target_os = "linux")]
    seen.check();
    #[cfg(target_os = "macos")]
    seen.check_darwin();
}

#[test]
fn a_client_closed_before_its_connection_is_accepted() {
    let seen = closed_before_accept(&mut Real::new(BACKEND));
    #[cfg(target_os = "linux")]
    seen.check();
    #[cfg(target_os = "macos")]
    seen.check_darwin();
}

#[test]
fn backpressure_stalls_a_sender_and_reading_resumes_it() {
    on(BACKEND, backpressure);
}

#[test]
fn a_cancel_of_a_waiting_accept() {
    on(BACKEND, cancel_accept);
}

#[test]
fn a_cancel_of_a_waiting_recv() {
    on(BACKEND, cancel_recv);
}

/// On the ring each race pairs one way: a cancel stops a target whose
/// process has not entered its ring since what it waited for arrived, and
/// is too late for one whose process has.
fn race(scenario: fn(&mut Real, Race) -> Cancelling) {
    for (race, pairing) in [
        (Race::CancelFirst, Pairing::Stopped),
        (Race::ArrivedAway, Pairing::Stopped),
        (Race::ArrivedEntered, Pairing::Completed),
    ] {
        let seen = scenario(&mut Real::new(BACKEND), race);
        seen.check();
        assert_eq!(seen.pairing(), pairing, "{race:?}: {seen:?}");
    }
}

#[test]
fn a_cancel_of_a_recv_racing_bytes() {
    race(cancel_recv_racing_bytes);
}

#[test]
fn a_cancel_of_an_accept_racing_a_connect() {
    race(cancel_accept_racing_a_connect);
}

#[test]
fn a_cancel_of_a_waiting_connect() {
    on(BACKEND, cancel_connect);
}

/// The SYN a full accept queue dropped is retransmitted a second later,
/// once an accept made room, and establishes the connection while the
/// client stays out of its ring: its cancel still stops the `Connect`.
#[test]
fn a_cancel_of_a_connect_established_while_its_client_was_away() {
    let seen = cancel_connect_established_while_away(&mut Real::new(BACKEND));
    seen.check();
    assert_eq!(seen.cancelling.pairing(), Pairing::Stopped, "{seen:?}");
}

#[test]
fn closing_a_listener_resets_the_connections_waiting_on_it() {
    on(BACKEND, listener_close_resets_waiting);
}
