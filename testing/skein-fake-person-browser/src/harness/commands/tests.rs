use super::*;
use skein_lib::Duration;
fn child(opens: bool) -> Process {
    let command = Command {
        program: "/fixture".into(),
        args: vec![],
        env: vec![],
        stdin: Stdin::Closed,
        opens: opens.then(|| Opens {
            prefix: b"http://".as_slice().into(),
            on: Output::Stdout,
            keep: b"link".as_slice().into(),
        }),
        within: Duration::from_secs(1),
    };
    let mut child = Process::new(
        0,
        command,
        Context { held: None, root: Fd::new(-1), address: "127.0.0.1:9", peers: &[], now: Time::ZERO, write_cap: 4096 },
    );
    child.requests.clear();
    child.event(io::Event::Spawned {
        owner: child.owner,
        child: Token::new(7),
        pipes: Box::new([Token::new(8), Token::new(9), Token::new(10)]),
    });
    child.requests.clear();
    child
}
fn bytes(child: &mut Process, output: Output, bytes: &[u8]) {
    child.event(io::Event::Stream {
        owner: Token::new(if output == Output::Stdout { 9 } else { 10 }),
        up: Up::Bytes(bytes.into()),
    });
}
#[test]
fn an_exit_before_output_does_not_close_the_output_pipe_or_lose_the_link() {
    let mut child = child(true);
    child.event(io::Event::Exited { owner: child.owner, exit: Exit::Code(0) });
    assert!(child.requests.is_empty());
    assert_eq!(child.advance(Time::ZERO, false, false), Progress::Idle);
    assert_eq!(child.advance(Time::ZERO, false, false), Progress::Idle);
    bytes(&mut child, Output::Stdout, b"http://127.0.0.1:9/whole\n");
    assert_eq!(child.advance(Time::ZERO, false, false), Progress::Open(b"http://127.0.0.1:9/whole".as_slice().into()));
    assert_eq!(child.output[0], b"http://127.0.0.1:9/whole\n");
}
#[test]
fn an_exited_commands_missing_line_waits_for_selected_eof() {
    let mut child = child(true);
    child.advance(Time::ZERO, false, false);
    child.event(io::Event::Exited { owner: child.owner, exit: Exit::Code(0) });
    assert_eq!(child.advance(Time::ZERO, false, false), Progress::Idle);
    child.event(io::Event::Stream { owner: Token::new(9), up: Up::End });
    assert_eq!(child.advance(Time::ZERO, false, false), Progress::Failed("exited without matching link".into()));
}
#[test]
fn command_output_at_the_named_bound_is_retained_whole() {
    let mut child = child(false);
    bytes(&mut child, Output::Stdout, &vec![b'x'; OUTPUT_BYTES]);
    assert_eq!(child.output[0].len(), OUTPUT_BYTES);
    assert_eq!(child.error, None);
}
#[test]
fn command_output_one_byte_past_the_bound_fails_before_appending() {
    let mut child = child(false);
    bytes(&mut child, Output::Stdout, &vec![b'\n'; OUTPUT_BYTES]);
    bytes(&mut child, Output::Stdout, b"x");
    assert_eq!(child.output[0].len(), OUTPUT_BYTES);
    assert_eq!(
        child.advance(Time::ZERO, false, false),
        Progress::Failed(format!("output exceeds {OUTPUT_BYTES} bytes"))
    );
}
#[test]
fn unterminated_command_output_overflow_is_bounded() {
    let mut child = child(true);
    bytes(&mut child, Output::Stdout, &vec![b'x'; OUTPUT_BYTES + 1]);
    assert!(child.output[0].is_empty());
    assert!(matches!(child.advance(Time::ZERO, false, false), Progress::Failed(_)));
}
#[test]
fn a_command_selects_stderr_without_reading_stdout_as_its_link() {
    let mut child = child(true);
    child.opens.as_mut().unwrap().on = Output::Stderr;
    bytes(&mut child, Output::Stdout, b"http://127.0.0.1:9/wrong\n");
    assert_eq!(child.link(), None);
    bytes(&mut child, Output::Stderr, b"http://127.0.0.1:9/right\n");
    assert_eq!(child.link().as_deref(), Some(b"http://127.0.0.1:9/right".as_slice()));
}
#[test]
fn a_command_link_waits_for_a_split_line_to_complete() {
    let mut child = child(true);
    bytes(&mut child, Output::Stdout, b"http://127.0.0.1:");
    assert_eq!(child.link(), None);
    bytes(&mut child, Output::Stdout, b"9/link\r\nhttp://127.0.0.1:9/later\n");
    assert_eq!(child.link().as_deref(), Some(b"http://127.0.0.1:9/link".as_slice()));
}
#[test]
fn a_missing_link_expires_at_the_commands_own_deadline() {
    let mut child = child(true);
    child.advance(Time::ZERO, false, false);
    let before = Time::ZERO.saturating_add(Duration::from_millis(999));
    assert_eq!(child.advance(before, false, false), Progress::Idle);
    assert_eq!(
        child.advance(Time::ZERO.saturating_add(Duration::from_secs(1)), false, false),
        Progress::Failed("Run missing-link/start deadline passed".into())
    );
    child.stop();
    assert!(child.requests.iter().any(|request| matches!(request, io::Request::Signal { signal: Signal::Kill, .. })));
}
#[test]
fn exited_expires_at_its_own_bound() {
    let mut child = child(false);
    child.wait = Some((Time::ZERO.saturating_add(Duration::from_millis(5)), WaitFor::Exit { code: 0 }));
    assert_eq!(child.advance(Time::ZERO.saturating_add(Duration::from_millis(4)), false, false), Progress::Idle);
    assert_eq!(
        child.advance(Time::ZERO.saturating_add(Duration::from_millis(5)), false, false),
        Progress::Failed("Exited deadline passed".into())
    );
    child.stop();
    assert!(
        child
            .requests
            .iter()
            .any(|request| matches!(request, io::Request::Close { entity } if *entity == Token::new(7)))
    );
}
#[test]
fn exited_refuses_a_signal_instead_of_an_expected_code() {
    let mut child = child(false);
    child.wait = Some((Time::ZERO.saturating_add(Duration::from_secs(1)), WaitFor::Exit { code: 0 }));
    child.event(io::Event::Exited { owner: child.owner, exit: Exit::Signal(9) });
    assert_eq!(child.advance(Time::ZERO, false, false), Progress::Failed("exited on signal 9, expected code 0".into()));
}

#[test]
fn command_substitution_preserves_unix_bytes_and_leaves_program_and_keys_literal() {
    use std::os::unix::ffi::OsStringExt;
    let command = Command {
        program: "/{neighbor}".into(),
        args: vec![std::ffi::OsString::from_vec(b"\xff{neighbor}/{address}/{neighbor}".to_vec())],
        env: vec![("{neighbor}".into(), std::ffi::OsString::from_vec(b"\xfe{address}/{neighbor}".to_vec()))],
        stdin: Stdin::Closed,
        opens: None,
        within: Duration::from_secs(1),
    };
    let mut child = Process::new(
        0,
        command,
        Context {
            held: None,
            root: Fd::new(-1),
            address: "127.0.0.1:8",
            peers: &[(b"neighbor".as_slice().into(), "127.0.0.1:9".into())],
            now: Time::ZERO,
            write_cap: 4096,
        },
    );
    let io::Request::Spawn { spawn, .. } = child.requests.pop_front().unwrap() else { panic!("spawn command") };
    assert_eq!(spawn.program.as_ref(), b"/{neighbor}");
    assert_eq!(spawn.args[0].as_ref(), b"\xff127.0.0.1:9/127.0.0.1:8/127.0.0.1:9");
    assert_eq!(spawn.env[0].as_ref(), b"{neighbor}=\xfe127.0.0.1:8/127.0.0.1:9");
}
#[test]
fn a_normal_exit_drains_both_pipes_before_closing_the_child() {
    let mut child = child(false);
    child.event(io::Event::Exited { owner: child.owner, exit: Exit::Code(0) });
    child.stop();
    assert!(child.requests.is_empty(), "normal exit keeps reading");
    bytes(&mut child, Output::Stderr, b"last diagnostic\n");
    for pipe in [8, 9, 10] {
        if pipe > 8 {
            child.event(io::Event::Stream { owner: Token::new(pipe), up: Up::End });
        }
        child.event(io::Event::Closed { owner: Token::new(pipe) });
    }
    assert!(
        child
            .requests
            .iter()
            .any(|request| matches!(request, io::Request::Close { entity } if *entity == Token::new(7)))
    );
    child.event(io::Event::Closed { owner: child.owner });
    assert!(child.settled && child.closed.iter().all(|closed| *closed));
    assert_eq!(child.stderr, b"last diagnostic\n");
}

#[test]
fn printed_stdin_sends_each_granted_chunk_then_finishes() {
    let command = Command {
        program: "/fixture".into(),
        args: vec![],
        env: vec![],
        stdin: Stdin::Printed,
        opens: None,
        within: Duration::from_secs(1),
    };
    let mut child = Process::new(
        0,
        command,
        Context {
            held: Some(b"link\n"),
            root: Fd::new(-1),
            address: "127.0.0.1:9",
            peers: &[],
            now: Time::ZERO,
            write_cap: 3,
        },
    );
    child.requests.clear();
    child.event(io::Event::Spawned {
        owner: child.owner,
        child: Token::new(7),
        pipes: Box::new([Token::new(8), Token::new(9), Token::new(10)]),
    });
    assert!(
        matches!(child.requests.front(), Some(io::Request::Stream { stream, down: Down::Demand { read: stream::Read::Nothing, room: 3 } }) if *stream == Token::new(8))
    );
    child.requests.clear();
    child.event(io::Event::Stream { owner: Token::new(8), up: Up::Room });
    assert_eq!(
        child.requests.pop_front(),
        Some(io::Request::Stream { stream: Token::new(8), down: Down::Send(b"lin".as_slice().into()) })
    );
    child.requests.clear();
    child.event(io::Event::Stream { owner: Token::new(8), up: Up::Room });
    assert_eq!(
        child.requests.pop_front(),
        Some(io::Request::Stream { stream: Token::new(8), down: Down::Send(b"k\n".as_slice().into()) })
    );
    assert_eq!(child.requests.pop_front(), Some(io::Request::Stream { stream: Token::new(8), down: Down::Finish }));
}
