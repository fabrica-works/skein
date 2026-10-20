use skein_browser::boundary::{Below, Down, Event, Expect, Key, Match, Point, Query, Refusal, Request, Trouble};
use skein_browser::wire::decode::Document;
use skein_browser::{down, fire, up};
use skein_lib::stream;
use skein_lib::{Time, Token};

use crate::world::{Fault, World};

#[test]
fn a_point_outside_the_box_is_hidden() {
    let mut world = World::new();
    world.start();
    world.open();
    for point in [Point { x: 100, y: 0 }, Point { x: 0, y: 30 }, Point { x: u32::MAX, y: u32::MAX }] {
        world.ask(Request::Press { page: Token::new(3), op: Token::new(4), node: 7, at: Some(point) });
        match world.above.pop() {
            Some(Event::Refused { op, why }) => {
                assert_eq!(op, Token::new(4));
                assert_eq!(why, Refusal::Hidden);
            }
            event => panic!("expected Hidden for {point:?}, got {event:?}"),
        }
    }
    assert!(world.hit_points.is_empty());
    assert!(!world.sent.iter().any(|command| {
        command.windows(b"Input.dispatchMouseEvent".len()).any(|part| part == b"Input.dispatchMouseEvent")
    }));
}

#[test]
fn a_press_at_a_point_hits_there() {
    let mut world = World::new();
    world.start();
    world.open();
    for (at, expected) in
        [(None, (60, 25)), (Some(Point { x: 0, y: 0 }), (10, 10)), (Some(Point { x: 99, y: 29 }), (109, 39))]
    {
        world.ask(Request::Press { page: Token::new(3), op: Token::new(4), node: 7, at });
        match world.above.pop() {
            Some(Event::Done { op }) => assert_eq!(op, Token::new(4)),
            event => panic!("expected Done for {at:?}, got {event:?}"),
        }
        assert_eq!(world.hit_points.last(), Some(&expected), "point {at:?}");
    }
}

#[test]
fn a_find_by_words_asks_without_a_name() {
    let mut world = World::with_limits(skein_browser::Limits { text: 1, ..skein_browser::Limits::default() });
    world.start();
    world.open();
    for (name, count) in [(b"av".as_slice(), 1), (b"save", 0), (b"", 1)] {
        world.ask(Request::Find {
            page: Token::new(3),
            op: Token::new(4),
            query: Query {
                role: Box::from(b"button".as_slice()),
                name: Box::from(name),
                matching: Match::Words,
                within: None,
                boxes: false,
            },
        });
        match world.above.pop() {
            Some(Event::Found { seen, more, .. }) => {
                assert_eq!(seen.len(), count, "query {name:?}");
                assert_eq!(more, 0);
                if let Some(node) = seen.get(0) {
                    assert_eq!(node.name.as_ref(), b"S");
                }
            }
            event => panic!("expected Found for {name:?}, got {event:?}"),
        }
    }
    assert_query_without_name(&world);
}

fn assert_query_without_name(world: &World) {
    let command = world
        .sent
        .iter()
        .rev()
        .find(|bytes| {
            bytes.windows(b"Accessibility.queryAXTree".len()).any(|part| part == b"Accessibility.queryAXTree")
        })
        .expect("query command");
    let document = Document::parse(&command[..command.len() - 1], world.env.limits.command).expect("command");
    let params = document.root().get(b"params").expect("params");
    assert_eq!(params.get(b"role").and_then(|value| value.text()).as_deref(), Some(b"button".as_slice()));
    assert!(params.get(b"accessibleName").is_none());
}

#[test]
fn lifecycle_find_and_press() {
    let mut world = World::new();
    world.start();
    world.open();
    world.ask(Request::Find {
        page: Token::new(3),
        op: Token::new(4),
        query: Query {
            role: b"button".to_vec().into_boxed_slice(),
            name: b"Save".to_vec().into_boxed_slice(),
            matching: Match::Exact,
            within: None,
            boxes: true,
        },
    });
    match world.above.pop() {
        Some(Event::Found { op, seen, more }) => {
            assert_eq!(op, Token::new(4));
            assert_eq!(more, 0);
            assert_eq!(seen.len(), 1);
            assert_eq!(seen.get(0).expect("button").node, 7);
            assert!(seen.get(0).expect("button").rect.is_some());
        }
        other => panic!("expected Found, got {other:?}"),
    }
    world.ask(Request::Press { page: Token::new(3), op: Token::new(5), node: 7, at: None });
    assert!(matches!(world.above.pop(), Some(Event::Done { op }) if op == Token::new(5)));
    let mouse = world
        .sent
        .iter()
        .filter(|command| command.windows(b"Input.dispatchMouseEvent".len()).any(|w| w == b"Input.dispatchMouseEvent"))
        .count();
    assert_eq!(mouse, 3);
}

#[test]
fn await_polls_until_present() {
    let mut world = World::new();
    world.start();
    world.open();
    world.ax_visible = false;
    world.ask(Request::Await {
        page: Token::new(3),
        op: Token::new(6),
        query: Query {
            role: b"button".to_vec().into_boxed_slice(),
            name: b"Save".to_vec().into_boxed_slice(),
            matching: Match::Exact,
            within: None,
            boxes: false,
        },
        expect: Expect::Present,
        within: skein_lib::Duration::from_millis(200),
    });
    assert!(world.above.is_empty());
    world.ax_visible = true;
    world.env.now = Time::from_nanos(50_000_000);
    fire(&mut world.browser, &world.env, world.env.now, &mut world.above, &mut world.below);
    world.drive();
    assert!(matches!(world.above.pop(), Some(Event::Met { op, .. }) if op == Token::new(6)));
}

#[test]
fn crash_fails_operation_and_closes_entities() {
    let mut world = World::new();
    world.start();
    world.open();
    world.ax_visible = false;
    world.ask(Request::Await {
        page: Token::new(3),
        op: Token::new(7),
        query: Query {
            role: b"missing".to_vec().into_boxed_slice(),
            name: b"No".to_vec().into_boxed_slice(),
            matching: Match::Exact,
            within: None,
            boxes: false,
        },
        expect: Expect::Present,
        within: skein_lib::Duration::from_secs(1),
    });
    up(&mut world.browser, &world.env, Below::Replies(stream::Up::End), &mut world.above, &mut world.below);
    let mut events = Vec::new();
    while let Some(event) = world.above.pop() {
        events.push(event);
    }
    for _ in 0..4 {
        fire(&mut world.browser, &world.env, world.env.now, &mut world.above, &mut world.below);
        while let Some(event) = world.above.pop() {
            events.push(event);
        }
    }
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Refused { op, why: Refusal::Crashed } if *op == Token::new(7)))
    );
    assert!(events.iter().any(|event| matches!(event, Event::Closed { owner } if *owner == Token::new(1))));
}

#[test]
fn await_misses_at_its_deadline() {
    let mut world = World::new();
    world.start();
    world.open();
    world.ax_visible = false;
    world.ask(Request::Await {
        page: Token::new(3),
        op: Token::new(8),
        query: Query {
            role: b"button".to_vec().into_boxed_slice(),
            name: b"Save".to_vec().into_boxed_slice(),
            matching: Match::Exact,
            within: None,
            boxes: false,
        },
        expect: Expect::Present,
        within: skein_lib::Duration::from_millis(100),
    });
    world.env.now = Time::from_nanos(100_000_000);
    fire(&mut world.browser, &world.env, world.env.now, &mut world.above, &mut world.below);
    world.drive();
    assert!(
        matches!(world.above.pop(), Some(Event::Missed { op, seen, more: 0 }) if op == Token::new(8) && seen.is_empty())
    );
}

#[test]
fn trouble_is_reported_without_judgment() {
    let mut world = World::new();
    world.start();
    world.open();
    let messages: [&[u8]; 3] = [
        b"{\"method\":\"Runtime.exceptionThrown\",\"sessionId\":\"s1\",\"params\":{\"exceptionDetails\":{\"text\":\"boom\"}}}\0",
        b"{\"method\":\"Runtime.consoleAPICalled\",\"sessionId\":\"s1\",\"params\":{\"type\":\"error\",\"args\":[{\"value\":\"bad\"}]}}\0",
        b"{\"method\":\"Log.entryAdded\",\"sessionId\":\"s1\",\"params\":{\"entry\":{\"level\":\"error\",\"text\":\"blocked\"}}}\0",
    ];
    for message in messages {
        up(
            &mut world.browser,
            &world.env,
            Below::Replies(stream::Up::Bytes(message.into())),
            &mut world.above,
            &mut world.below,
        );
    }
    assert!(
        matches!(world.above.pop(), Some(Event::Trouble { trouble: Trouble::Exception, text, .. }) if text.as_ref() == b"boom")
    );
    assert!(
        matches!(world.above.pop(), Some(Event::Trouble { trouble: Trouble::Console, text, .. }) if text.as_ref() == b"bad")
    );
    assert!(
        matches!(world.above.pop(), Some(Event::Trouble { trouble: Trouble::Log, text, .. }) if text.as_ref() == b"blocked")
    );
    world.ask(Request::Screenshot { page: Token::new(3), op: Token::new(9) });
    assert!(
        matches!(world.above.pop(), Some(Event::Screenshot { op, png }) if op == Token::new(9) && png.starts_with(b"\x89PNG"))
    );
}

#[test]
fn press_refuses_disabled_hidden_and_covered_without_mouse() {
    for (disabled, hidden, covered, why) in [
        (true, false, false, Refusal::Disabled),
        (false, true, false, Refusal::Hidden),
        (false, false, true, Refusal::Covered),
    ] {
        let mut world = World::new();
        world.start();
        world.open();
        world.disabled = disabled;
        world.hidden = hidden;
        world.covered = covered;
        world.ask(Request::Press { page: Token::new(3), op: Token::new(10), node: 7, at: None });
        assert!(
            matches!(world.above.pop(), Some(Event::Refused { op, why: observed }) if op == Token::new(10) && observed == why)
        );
        assert!(!world.sent.iter().any(|command| {
            command.windows(b"Input.dispatchMouseEvent".len()).any(|window| window == b"Input.dispatchMouseEvent")
        }));
    }
}

#[test]
fn unanswered_command_reaches_answer_deadline() {
    let mut world = World::new();
    world.start();
    world.open();
    down(
        &mut world.browser,
        &world.env,
        Request::Snapshot { page: Token::new(3), op: Token::new(11) },
        &mut world.above,
        &mut world.below,
    );
    let id = world
        .below
        .iter()
        .find_map(|record| match record {
            Down::Commands(stream::Down::Send(bytes)) => {
                let document =
                    Document::parse(&bytes[..bytes.len() - 1], world.env.limits.command).expect("valid command");
                document.root().get(b"id").and_then(|id| id.u64())
            }
            _ => None,
        })
        .expect("snapshot command was sent");
    world.env.now = Time::from_nanos(world.env.limits.answer.as_nanos());
    fire(&mut world.browser, &world.env, world.env.now, &mut world.above, &mut world.below);
    assert!(matches!(world.above.pop(), Some(Event::Refused { op, why: Refusal::Timeout }) if op == Token::new(11)));
    let late = format!("{{\"id\":{id},\"result\":{{}}}}\0");
    up(
        &mut world.browser,
        &world.env,
        Below::Replies(stream::Up::Bytes(late.into_bytes().into_boxed_slice())),
        &mut world.above,
        &mut world.below,
    );
    assert!(world.above.is_empty(), "a late reply cannot answer twice or close the browser");
}

#[test]
fn node_disappearing_during_press_is_refused_before_mouse() {
    let mut world = World::new();
    world.start();
    world.open();
    world.fault(b"DOM.getContentQuads", Fault::Error);
    world.ask(Request::Press { page: Token::new(3), op: Token::new(12), node: 7, at: None });
    assert!(matches!(world.above.pop(), Some(Event::Refused { op, why: Refusal::Gone }) if op == Token::new(12)));
    assert!(!world.sent.iter().any(|command| {
        command.windows(b"Input.dispatchMouseEvent".len()).any(|part| part == b"Input.dispatchMouseEvent")
    }));
    world.assert_commands_settled();
}

#[test]
fn page_crash_mid_operation_reports_trouble_and_one_refusal() {
    let mut world = World::new();
    world.start();
    world.open();
    world.fault(b"Accessibility.getFullAXTree", Fault::PageCrash);
    world.ask(Request::Snapshot { page: Token::new(3), op: Token::new(13) });
    let mut trouble = 0;
    let mut refused = 0;
    while let Some(event) = world.above.pop() {
        match event {
            Event::Trouble { trouble: Trouble::Crash, .. } => trouble += 1,
            Event::Refused { op, why: Refusal::Crashed } if op == Token::new(13) => refused += 1,
            Event::Closed { .. } => {}
            other => panic!("unexpected event after page crash: {other:?}"),
        }
    }
    assert_eq!((trouble, refused), (1, 1));
    world.assert_commands_settled();
}

mod navigation;

fn native_dialog(world: &mut World, kind: &str, message: &str) {
    let bytes = format!(
        r#"{{"method":"Page.javascriptDialogOpening","sessionId":"s1","params":{{"type":"{kind}","message":"{message}"}}}}"#
    );
    let mut bytes = bytes.into_bytes();
    bytes.push(0);
    up(
        &mut world.browser,
        &world.env,
        Below::Replies(stream::Up::Bytes(bytes.into())),
        &mut world.above,
        &mut world.below,
    );
}
fn dialog_answer(world: &World) -> bool {
    let command = world
        .sent
        .iter()
        .rev()
        .find(|bytes| bytes.windows(27).any(|part| part == b"Page.handleJavaScriptDialog"))
        .expect("dialog answered");
    let document = Document::parse(&command[..command.len() - 1], world.env.limits.command).expect("answer command");
    document
        .root()
        .get(b"params")
        .expect("parameters")
        .get(b"accept")
        .and_then(|value| value.bool())
        .expect("accept boolean")
}
#[test]
fn a_go_accepts_its_pages_closing_warning() {
    let mut world = World::new();
    world.start();
    world.open();
    let go = Token::new(8);
    down(
        &mut world.browser,
        &world.env,
        Request::Go { page: Token::new(3), op: go, to: skein_browser::boundary::Go::Reload },
        &mut world.above,
        &mut world.below,
    );
    native_dialog(&mut world, "beforeunload", "");
    match world.above.pop() {
        Some(Event::Warned { op }) => assert_eq!(op, go),
        other => panic!("expected Warned, got {other:?}"),
    }
    native_dialog(&mut world, "beforeunload", "");
    assert!(world.above.is_empty(), "one warning per go");
    world.drive();
    assert!(dialog_answer(&world));
    match world.above.pop() {
        Some(Event::Done { op }) => assert_eq!(op, go),
        other => panic!("expected Done, got {other:?}"),
    }
}
const DISMISSED_DIALOGS: &[(&str, &str, &[u8])] =
    &[("confirm", "Delete it?", b"confirm: Delete it?"), ("beforeunload", "", b"beforeunload: ")];
#[test]
fn a_dialog_outside_a_go_is_dismissed_as_trouble() {
    for &(kind, message, expected) in DISMISSED_DIALOGS {
        let mut world = World::new();
        world.start();
        world.open();
        native_dialog(&mut world, kind, message);
        match world.above.pop() {
            Some(Event::Trouble { page, trouble, text }) => {
                assert_eq!(page, Token::new(3));
                assert_eq!(trouble, Trouble::Dialog);
                assert_eq!(text.as_ref(), expected, "{kind}");
            }
            other => panic!("expected dialog trouble, got {other:?}"),
        }
        world.drive();
        assert!(!dialog_answer(&world), "{kind}");
    }
}

const DIALOG_TEXT_LIMITS: &[(u32, &[u8])] = &[(0, b""), (5, b"confi"), (19, b"confirm: Delete it?")];
#[test]
fn a_native_dialogs_trouble_text_is_cut_to_its_bound() {
    for &(limit, expected) in DIALOG_TEXT_LIMITS {
        let mut world = World::with_limits(skein_browser::Limits { text: limit, ..skein_browser::Limits::default() });
        world.start();
        world.open();
        native_dialog(&mut world, "confirm", "Delete it?");
        match world.above.pop() {
            Some(Event::Trouble { text, .. }) => assert_eq!(text.as_ref(), expected, "limit {limit}"),
            other => panic!("expected dialog trouble, got {other:?}"),
        }
    }
}

const FIELD: u64 = 7; // the world's sole backend node
const PART_KEYS: &[&str] = &[
    r#"DOM.focus {"backendNodeId":7}"#,
    r#"Input.dispatchKeyEvent {"type":"keyDown","key":"0","code":"Digit0","windowsVirtualKeyCode":48,"text":"0"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyUp","key":"0","code":"Digit0","windowsVirtualKeyCode":48,"text":"0"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyDown","key":"2","code":"Digit2","windowsVirtualKeyCode":50,"text":"2"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyUp","key":"2","code":"Digit2","windowsVirtualKeyCode":50,"text":"2"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyDown","key":"3","code":"Digit3","windowsVirtualKeyCode":51,"text":"3"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyUp","key":"3","code":"Digit3","windowsVirtualKeyCode":51,"text":"3"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyDown","key":"0","code":"Digit0","windowsVirtualKeyCode":48,"text":"0"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyUp","key":"0","code":"Digit0","windowsVirtualKeyCode":48,"text":"0"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyDown","key":"P","code":"KeyP","windowsVirtualKeyCode":80,"text":"P"}"#,
    r#"Input.dispatchKeyEvent {"type":"keyUp","key":"P","code":"KeyP","windowsVirtualKeyCode":80,"text":"P"}"#,
];
fn key_commands(world: &World) -> Vec<String> {
    world
        .sent
        .iter()
        .map(|command| {
            let document = Document::parse(&command[..command.len() - 1], world.env.limits.command).unwrap();
            let root = document.root();
            let method = root.get(b"method").unwrap().text().unwrap();
            let params = root.get(b"params").unwrap().raw();
            format!("{} {}", String::from_utf8_lossy(&method), String::from_utf8_lossy(params))
        })
        .collect()
}
fn keyed(text: &[u8]) -> World {
    let mut world = World::new();
    world.start();
    world.open();
    world.sent.clear();
    world.ask(Request::Keys { page: Token::new(3), op: Token::new(4), node: FIELD, text: text.into() });
    world
}
#[test]
fn keys_type_each_character_as_its_key_down_and_up() {
    let mut world = keyed(b"0230P");
    assert_eq!(key_commands(&world), PART_KEYS);
    match world.above.pop() {
        Some(Event::Done { op }) => assert_eq!(op, Token::new(4)),
        other => panic!("expected Done, got {other:?}"),
    }
}
fn enter_sent(shift: bool) -> World {
    let mut world = World::new();
    world.start();
    world.open();
    world.sent.clear();
    world.ask(Request::Key { page: Token::new(3), op: Token::new(4), key: Key::Enter, shift });
    world
}
#[test]
fn a_key_with_shift_held_sends_shifts_modifier_on_both_commands() {
    let shifted: Vec<String> = key_commands(&enter_sent(false))
        .iter()
        .map(|command| format!("{}{}", &command[..command.len() - 1], r#","modifiers":8}"#))
        .collect();
    let mut world = enter_sent(true);
    assert_eq!(key_commands(&world), shifted);
    match world.above.pop() {
        Some(Event::Done { op }) => assert_eq!(op, Token::new(4)),
        other => panic!("expected Done, got {other:?}"),
    }
}
#[test]
fn a_key_without_shift_sends_no_modifiers() {
    let commands = key_commands(&enter_sent(false));
    assert_eq!(commands.len(), 2);
    assert!(commands.iter().all(|command| !command.contains("modifiers")), "{commands:?}");
}
#[test]
fn empty_keys_focus_without_typing() {
    let mut world = keyed(b"");
    assert_eq!(key_commands(&world), [r#"DOM.focus {"backendNodeId":7}"#]);
    match world.above.pop() {
        Some(Event::Done { op }) => assert_eq!(op, Token::new(4)),
        other => panic!("expected Done, got {other:?}"),
    }
}
#[test]
fn keys_type_unicode_characters_without_a_physical_code() {
    let world = keyed("é".as_bytes());
    assert_eq!(
        key_commands(&world),
        [
            r#"DOM.focus {"backendNodeId":7}"#,
            r#"Input.dispatchKeyEvent {"type":"keyDown","key":"é","code":"","windowsVirtualKeyCode":0,"text":"é"}"#,
            r#"Input.dispatchKeyEvent {"type":"keyUp","key":"é","code":"","windowsVirtualKeyCode":0,"text":"é"}"#,
        ]
    );
}
#[test]
fn keys_refuse_text_that_is_not_utf8() {
    let mut world = keyed(&[0xff]);
    match world.above.pop() {
        Some(Event::Refused { why, .. }) => assert_eq!(why, Refusal::Protocol),
        other => panic!("expected Protocol, got {other:?}"),
    }
}

mod attribute;
