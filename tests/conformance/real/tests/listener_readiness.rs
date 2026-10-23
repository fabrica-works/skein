//! The readiness wait observes the listener without consuming its connection.

use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;

use skein_io::kernel::Fd;
use skein_lib::Duration;
use skein_shell::{Clock, wait_readable};

#[test]
fn an_empty_listener_times_out() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("a loopback listener");
    let clock = Clock::new();
    let before = clock.now().now;
    let bound = Duration::from_millis(10);
    assert_eq!(wait_readable(Fd::new(listener.as_raw_fd()), bound), Ok(false), "no connection is queued");
    assert!(clock.now().now.saturating_since(before) >= bound, "the wait lasts until its deadline");
}

#[test]
fn a_queued_connection_stays_available_after_waiting() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("a loopback listener");
    listener.set_nonblocking(true).expect("accept must not block if readiness consumed the connection");
    let _client = TcpStream::connect(listener.local_addr().expect("the listener's address")).expect("a connection");
    let fd = Fd::new(listener.as_raw_fd());
    for _ in 0..2 {
        assert_eq!(wait_readable(fd, Duration::from_secs(1)), Ok(true), "the connection remains queued");
    }
    let _accepted = listener.accept().expect("the wait did not consume the connection");
    assert_eq!(wait_readable(fd, Duration::ZERO), Ok(false), "accept consumed the one connection");
}

#[test]
fn an_invalid_descriptor_is_an_error() {
    for raw in [-1, i32::MAX] {
        assert_eq!(wait_readable(Fd::new(raw), Duration::ZERO), Err(libc::EBADF), "an invalid fd is not ready");
    }
}
