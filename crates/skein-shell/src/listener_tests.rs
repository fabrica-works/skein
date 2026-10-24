//! Real-kernel startup listener checks shared by both system boundaries.

use std::net::SocketAddr;
use std::os::unix::ffi::OsStrExt;
use std::ptr;

use crate::system;
use crate::{ListenersRefusal, MAX_INHERITED_LISTENERS, inherited_listeners, listener_at, open_listener};
use skein_io::kernel::{Error, Family, Fd};

fn addr() -> SocketAddr {
    "127.0.0.1:0".parse().expect("loopback address")
}

fn close(fd: Fd) {
    system::close(fd).expect("close test descriptor");
}

fn flags(fd: Fd) -> i32 {
    // SAFETY: F_GETFD takes only an integer descriptor and changes nothing.
    unsafe { libc::fcntl(fd.raw(), libc::F_GETFD) }
}

fn duplicate_high(fd: Fd) -> Fd {
    // SAFETY: F_DUPFD_CLOEXEC takes integers and returns an independently owned fd.
    let raw = unsafe { libc::fcntl(fd.raw(), libc::F_DUPFD_CLOEXEC, 64) };
    assert!(raw >= 64, "save test descriptor away from activation slots");
    Fd::new(raw)
}

fn move_high(fd: Fd) -> Fd {
    let high = duplicate_high(fd);
    close(fd);
    high
}

fn install(source: Fd, target: i32) {
    // SAFETY: source is open and target is a saved or vacant activation slot in this child only.
    assert_eq!(unsafe { libc::dup2(source.raw(), target) }, target, "install activation descriptor");
    close(source);
}

fn pipe() -> (Fd, Fd) {
    let mut fds = [0_i32; 2];
    // SAFETY: fds is exclusive initialized storage for exactly two descriptors.
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "open test pipe");
    let [read, write] = fds;
    (move_high(Fd::new(read)), move_high(Fd::new(write)))
}

// Environment changes use Command's child environment, never set_var. A child
// runs exactly one test. It saves any harness descriptors at 3 and 4 above 63,
// closes those slots, and restores them after the check. Fixtures also move
// above 63 before dup2, so neither activation slot can clobber a fixture.
#[expect(clippy::disallowed_types, reason = "a child isolates activation fds and environment from the test harness")]
fn inherited_case(name: &str, count: Option<&str>, check: impl FnOnce()) {
    if std::env::var("SKEIN_LISTENER_TEST").ok().as_deref() == Some(name) {
        let saved = [3, 4].map(|raw| {
            let fd = Fd::new(raw);
            if flags(fd) < 0 {
                None
            } else {
                let original_flags = flags(fd);
                let saved = duplicate_high(fd);
                close(fd);
                Some((saved, original_flags))
            }
        });
        check();
        for (raw, saved) in [3, 4].into_iter().zip(saved) {
            let fd = Fd::new(raw);
            if flags(fd) >= 0 {
                close(fd);
            }
            if let Some((saved, original_flags)) = saved {
                install(saved, raw);
                // SAFETY: restore the saved descriptor's integer flags in this child.
                assert_eq!(unsafe { libc::fcntl(raw, libc::F_SETFD, original_flags) }, 0, "restore harness flags");
            }
        }
        return;
    }
    let module = module_path!().split_once("::").expect("crate and module").1;
    let mut command = std::process::Command::new(std::env::current_exe().expect("test binary"));
    command
        .args(["--exact", &format!("{module}::{name}"), "--nocapture", "--test-threads=1"])
        .env("SKEIN_LISTENER_TEST", name)
        .env("LISTEN_PID", "not-a-pid");
    match count {
        Some(count) => {
            command.env("LISTEN_FDS", count);
        }
        None => {
            command.env_remove("LISTEN_FDS");
        }
    }
    let output = command.output().expect("run isolated activation check");
    assert!(
        output.status.success(),
        "child failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn inherited_none() {
    inherited_case("inherited_none", None, || {
        let (fd, _) = open_listener(addr(), 8).expect("listener");
        install(move_high(fd), 3);
        assert!(inherited_listeners().expect("absent count").is_empty(), "no inherited listeners");
        assert_eq!(flags(Fd::new(3)), 0, "absent count leaves fd 3 alone");
    });
}

#[test]
fn inherited_empty() {
    inherited_case("inherited_empty", Some(""), || {
        assert!(inherited_listeners().expect("empty count").is_empty(), "no inherited listeners");
    });
}

#[test]
fn inherited_zero() {
    inherited_case("inherited_zero", Some("0"), || {
        assert!(inherited_listeners().expect("zero count").is_empty(), "no inherited listeners");
    });
}

#[test]
fn inherited_two() {
    inherited_case("inherited_two", Some("2"), || {
        let (first, first_addr) = open_listener(addr(), 8).expect("first listener");
        let first = move_high(first);
        let (second, second_addr) = open_listener(addr(), 8).expect("second listener");
        let second = move_high(second);
        install(first, 3);
        install(second, 4);
        let inherited = inherited_listeners().expect("two listeners");
        assert_eq!(
            &*inherited,
            &[(Fd::new(3), first_addr), (Fd::new(4), second_addr)],
            "descriptor order and addresses"
        );
        for (fd, _) in &inherited {
            assert_ne!(flags(*fd) & libc::FD_CLOEXEC, 0, "close-on-exec set");
        }
        assert_eq!(std::env::var("LISTEN_FDS").expect("count retained"), "2", "environment retained");
    });
}

#[test]
fn inherited_bad_count() {
    inherited_case("inherited_bad_count", Some("+2"), || {
        assert_eq!(inherited_listeners(), Err(ListenersRefusal::BadCount));
    });
}

#[test]
fn inherited_past_bound() {
    inherited_case("inherited_past_bound", Some("1025"), || {
        assert_eq!(inherited_listeners(), Err(ListenersRefusal::TooMany { bound: MAX_INHERITED_LISTENERS }));
    });
}

#[test]
fn inherited_missing_fd() {
    inherited_case("inherited_missing_fd", Some("1"), || {
        assert_eq!(inherited_listeners(), Err(ListenersRefusal::NotOpen { fd: Fd::new(3) }));
    });
}

#[test]
fn inherited_pipe() {
    inherited_case("inherited_pipe", Some("1"), || {
        let (read, write) = pipe();
        install(read, 3);
        assert!(
            matches!(inherited_listeners(), Err(ListenersRefusal::NotSocket { fd, mode })
            if fd == Fd::new(3) && u64::from(mode) & u64::from(libc::S_IFMT) == u64::from(libc::S_IFIFO)),
            "pipe identified at fd 3"
        );
        close(write);
    });
}

#[test]
fn inherited_bound_not_listening() {
    inherited_case("inherited_bound_not_listening", Some("1"), || {
        let fd = system::socket(Family::Ipv4).expect("socket");
        system::bind(fd, &addr()).expect("bound socket");
        install(move_high(fd), 3);
        assert_eq!(inherited_listeners(), Err(ListenersRefusal::NotListening { fd: Fd::new(3) }));
    });
}

#[test]
fn listener_at_preserves_flags() {
    let (fd, bound) = open_listener(addr(), 8).expect("listener");
    let status = system::status_flags(fd.raw()).expect("status flags");
    for descriptor_flags in [0, libc::FD_CLOEXEC] {
        // SAFETY: F_SETFD changes only this test's owned descriptor flags.
        assert_eq!(unsafe { libc::fcntl(fd.raw(), libc::F_SETFD, descriptor_flags) }, 0, "set flags");
        assert_eq!(listener_at(fd), Ok(bound));
        assert_eq!(flags(fd), descriptor_flags, "descriptor flags preserved");
        assert_eq!(system::status_flags(fd.raw()), Ok(status), "status flags preserved");
    }
    close(fd);
}

#[test]
fn listener_at_pipe() {
    let (read, write) = pipe();
    assert!(matches!(listener_at(read), Err(ListenersRefusal::NotSocket { fd, .. }) if fd == read), "pipe refused");
    close(read);
    close(write);
}

#[test]
fn listener_at_datagram() {
    // SAFETY: socket takes integers and returns a descriptor owned by this test.
    let raw = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    assert!(raw >= 0, "datagram socket opens");
    let fd = Fd::new(raw);
    assert_eq!(listener_at(fd), Err(ListenersRefusal::NotStream { fd, kind: libc::SOCK_DGRAM }));
    close(fd);
}

#[test]
fn listener_at_unix() {
    // SAFETY: socket takes integers and returns a descriptor owned by this test.
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    assert!(raw >= 0, "Unix socket opens");
    let fd = Fd::new(raw);
    // An unnamed Unix socket can listen on Linux but Darwin requires a bind.
    let scratch = skein_scratch::Scratch::new("listener-unix");
    let path = scratch.path().join("sock");
    // SAFETY: sockaddr_un is a plain integer/character C record valid at zero.
    let mut local: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    local.sun_family = libc::sa_family_t::try_from(libc::AF_UNIX).expect("family fits");
    for (slot, byte) in local.sun_path.iter_mut().zip(path.as_os_str().as_bytes()) {
        *slot = libc::c_char::try_from(*byte).expect("ASCII scratch path");
    }
    let len = libc::socklen_t::try_from(size_of::<libc::sockaddr_un>()).expect("address size fits");
    #[cfg(target_os = "macos")]
    {
        local.sun_len = u8::try_from(len).expect("Unix address size fits");
    }
    // SAFETY: local is initialized and readable for len bytes until bind returns.
    assert_eq!(unsafe { libc::bind(raw, ptr::from_ref(&local).cast(), len) }, 0, "bind Unix socket");
    system::listen(fd, 8).expect("Unix listener");
    assert_eq!(listener_at(fd), Err(ListenersRefusal::NotIp { fd }));
    close(fd);
}

#[test]
fn open_listener_taken_port() {
    let (fd, bound) = open_listener(addr(), 8).expect("first listener");
    assert_eq!(open_listener(bound, 8), Err(Error::AddressInUse));
    close(fd);
}

#[test]
fn open_listener_port_zero() {
    let (fd, bound) = open_listener(addr(), 8).expect("ephemeral listener");
    assert_ne!(bound.port(), 0, "kernel selected a port");
    assert_eq!(bound.ip(), addr().ip(), "requested interface");
    assert_eq!(listener_at(fd), Ok(bound), "returned address belongs to a listener");
    assert_ne!(flags(fd) & libc::FD_CLOEXEC, 0, "close-on-exec set");
    assert_ne!(system::status_flags(fd.raw()).expect("status flags") & libc::O_NONBLOCK, 0, "nonblocking like io");
    assert_eq!(super::option(fd, libc::SO_REUSEADDR), Ok(1), "io's bind option");
    close(fd);
}

#[test]
fn open_listener_ipv6() {
    let requested = "[::1]:0".parse().expect("IPv6 loopback");
    let (fd, bound) = open_listener(requested, 8).expect("IPv6 listener");
    assert_ne!(bound.port(), 0, "kernel selected a port");
    assert_eq!(bound.ip(), requested.ip());
    assert_eq!(listener_at(fd), Ok(bound));
    close(fd);
}
