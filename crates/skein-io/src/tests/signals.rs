//! Signal intake through io's public entry points (io.md, section 7).

use super::{Kind, Rig, limits};
use crate::kernel::{Done, Error, Fd, Op, ServiceSignal};
use crate::{Event, Limits, Request};

#[test]
fn signal_source_rearms_and_settles_its_read_before_close() {
    let mut rig = Rig::new(Limits { sockets: 1, ..limits() });
    let source = rig.io.adopt_signals(Fd::new(50)).expect("one entity slot");
    assert_eq!(rig.io.adopt_signals(Fd::new(51)), Err(Fd::new(51)));
    let read = rig.next().take(Kind::ReadSignal);
    assert_eq!(read.kind, Op::ReadSignal { fd: Fd::new(50) });

    let mut out =
        rig.complete(read, Ok(Done::Signalled(crate::kernel::Signalled::Termination(ServiceSignal::Interrupt))));
    assert_eq!(out.events, [Event::Shutdown { signal: ServiceSignal::Interrupt }]);
    let read = out.take(Kind::ReadSignal);
    let mut out =
        rig.complete(read, Ok(Done::Signalled(crate::kernel::Signalled::Termination(ServiceSignal::Terminate))));
    assert_eq!(out.events, [Event::Shutdown { signal: ServiceSignal::Terminate }]);
    let read = out.take(Kind::ReadSignal);

    let cancel = rig.down(Request::Close { entity: source }).take(Kind::Cancel);
    rig.complete(cancel, Ok(Done::Nothing)).nothing();
    let close = rig.complete(read, Err(Error::Cancelled)).take(Kind::Close);
    assert_eq!(close.kind, Op::Close { fd: Fd::new(50) });
    assert_eq!(rig.complete(close, Ok(Done::Nothing)).events, [Event::Closed { owner: source }]);
    rig.empty();
}

#[test]
fn signal_failure_and_close_echo_the_service_binding_instead_of_its_io_handle() {
    let mut rig = Rig::new(Limits { sockets: 1, ..limits() });
    let owner = super::owner(9);
    let source = rig.io.adopt_signals_for(Fd::new(50), owner).expect("one entity slot");
    assert_ne!(source, owner, "the io handle and the service binding are independent");
    assert_eq!(rig.io.adopt_signals_for(Fd::new(51), owner), Err(Fd::new(51)));
    let read = rig.next().take(Kind::ReadSignal);
    let mut out = rig.complete(read, Err(Error::Permission));
    assert_eq!(out.events, [Event::Failed { owner, error: crate::Error::Other }]);
    let close = out.take(Kind::Close);
    assert_eq!(rig.complete(close, Ok(Done::Nothing)).events, [Event::Closed { owner }]);
    rig.empty();
}

#[test]
fn a_signal_read_retries_an_unsubmitted_cancel_only_while_its_target_waits() {
    for read_first in [false, true] {
        let mut rig = Rig::new(Limits { sockets: 1, ..limits() });
        let source = rig.io.adopt_signals(Fd::new(50)).expect("one entity slot");
        let read = rig.next().take(Kind::ReadSignal);
        let cancel = rig.down(Request::Close { entity: source }).take(Kind::Cancel);
        if read_first {
            rig.complete(read, Ok(Done::Signalled(crate::kernel::Signalled::Termination(ServiceSignal::Terminate))))
                .nothing();
            let close = rig.complete(cancel, Err(Error::Other(11))).take(Kind::Close);
            rig.complete(close, Ok(Done::Nothing));
        } else {
            let retried = rig.complete(cancel, Err(Error::Other(11))).take(Kind::Cancel);
            assert_eq!(retried.kind, Op::Cancel { target: read.op });
            rig.complete(retried, Ok(Done::Nothing)).nothing();
            let close = rig.complete(read, Err(Error::Cancelled)).take(Kind::Close);
            rig.complete(close, Ok(Done::Nothing));
        }
        rig.empty();
    }
}

#[test]
fn a_window_change_is_told_as_resized_and_never_as_shutdown() {
    let mut rig = Rig::new(limits());
    let source = rig.io.adopt_signals(Fd::new(50)).expect("signal source");
    let read = rig.next().take(Kind::ReadSignal);
    let mut out = rig.complete(read, Ok(Done::Signalled(crate::kernel::Signalled::WindowChanged)));
    assert_eq!(out.events, [Event::Resized]);
    let read = out.take(Kind::ReadSignal);
    let cancel = rig.down(Request::Close { entity: source }).take(Kind::Cancel);
    rig.complete(cancel, Ok(Done::Nothing)).nothing();
    let close = rig.complete(read, Err(Error::Cancelled)).take(Kind::Close);
    rig.complete(close, Ok(Done::Nothing));
    rig.empty();
}

#[test]
fn a_window_request_has_one_terminal() {
    let mut rig = Rig::new(limits());
    let stream = rig.io.adopt_read_pipe(Fd::new(60)).expect("input stream");
    let read = rig.next().take(Kind::PipeRead);
    let window = rig.down(Request::Window { owner: super::owner(7), stream }).take(Kind::Window);
    assert_eq!(window.kind, Op::Window { fd: Fd::new(60) });
    let size = crate::kernel::Size { rows: 30, columns: 100 };
    let out = rig.complete(window, Ok(Done::Window(size)));
    assert_eq!(out.events, [Event::Window { owner: super::owner(7), window: crate::Window::Size(size) }]);
    let cancel = rig.down(Request::Abort { entity: stream }).take(Kind::Cancel);
    rig.complete(cancel, Ok(Done::Nothing));
    let close = rig.complete(read, Err(Error::Cancelled)).take(Kind::Close);
    rig.complete(close, Ok(Done::Nothing));
    rig.empty();
}

#[test]
fn a_window_on_a_terminal_that_hung_up_is_gone() {
    let mut rig = Rig::new(limits());
    let stream = rig.io.adopt_read_pipe(Fd::new(60)).expect("input stream");
    let read = rig.next().take(Kind::PipeRead);
    let window = rig.down(Request::Window { owner: super::owner(7), stream }).take(Kind::Window);
    assert_eq!(
        rig.complete(window, Err(Error::Other(5))).events,
        [Event::Window { owner: super::owner(7), window: crate::Window::Gone }]
    );
    let cancel = rig.down(Request::Abort { entity: stream }).take(Kind::Cancel);
    rig.complete(cancel, Ok(Done::Nothing));
    let close = rig.complete(read, Err(Error::Cancelled)).take(Kind::Close);
    rig.complete(close, Ok(Done::Nothing));
    rig.empty();
}
