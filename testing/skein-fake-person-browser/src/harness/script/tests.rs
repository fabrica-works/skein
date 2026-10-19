use super::{Play, Script, Step};
use crate::BrowserFace;
use crate::tests::{face, found, seen};
use skein_browser::boundary::{Event, Key, Request};
use skein_fake_person::Find;
use skein_lib::{Time, Token};
use skein_ui::Role;

#[test]
fn trouble_named_by_a_later_step_does_not_fail_the_run() {
    let mut script = Script::new(Box::new([
        Step::Key(Key::Escape),
        Step::Trouble { words: b"frame-ancestors".as_slice().into(), within: skein_lib::Duration::from_secs(1) },
    ]));
    assert_eq!(script.expected(), vec![Box::from(b"frame-ancestors".as_slice())]);
    let mut face = face();
    face.event(&skein_browser::boundary::Event::Trouble {
        page: Token::new(8),
        trouble: skein_browser::boundary::Trouble::Exception,
        text: b"frame-ancestors".as_slice().into(),
    });
    assert_eq!(face.trouble(), &[]);
    face.event(&skein_browser::boundary::Event::Trouble {
        page: Token::new(7),
        trouble: skein_browser::boundary::Trouble::Exception,
        text: b"frame-ancestors".as_slice().into(),
    });
    script.next(&mut face, Time::ZERO);
    script.next(&mut face, Time::ZERO);
    script.next(&mut face, Time::ZERO);
    assert_eq!(script.at(), 2);
    assert_eq!(face.failure(), None);
}
#[test]
fn a_trouble_step_not_met_within_its_bound_fails_naming_its_words() {
    let mut script = Script::new(Box::new([Step::Trouble {
        words: b"frame-ancestors".as_slice().into(),
        within: skein_lib::Duration::from_secs(1),
    }]));
    let mut face = face();
    script.next(&mut face, Time::ZERO);
    script.check_deadline(&mut face, Time::ZERO.saturating_add(skein_lib::Duration::from_secs(1)));
    assert_eq!(face.failure().as_deref(), Some("did not receive trouble containing frame-ancestors within its bound"));
}
#[test]
fn a_request_step_with_another_status_fails_naming_it() {
    let mut script = Script::new(Box::new([Step::Request {
        method: skein_http::Method::Get,
        path: b"/".as_slice().into(),
        host: b"other.test".as_slice().into(),
        headers: vec![],
        body: Box::new([]),
        status: 200,
        within: skein_lib::Duration::from_secs(1),
    }]));
    let mut face = face();
    script.next(&mut face, Time::ZERO);
    assert_eq!(script.request().unwrap().host.as_ref(), b"other.test");
    script.answered(403);
    script.check_deadline(&mut face, Time::ZERO.saturating_add(skein_lib::Duration::from_secs(1)));
    assert_eq!(face.failure().as_deref(), Some("did not receive status 200, saw Some(403) within its bound"));
}
#[test]
fn a_key_step_sends_the_kits_key() {
    let mut script = Script::new(Box::new([Step::Key(Key::Escape)]));
    let mut face = face();
    script.next(&mut face, Time::ZERO);
    assert_eq!(
        face.requests().collect::<Vec<_>>(),
        vec![Request::Key { page: Token::new(7), op: Token::new(7_000_001), key: Key::Escape, shift: false }]
    );
}
#[test]
fn a_key_with_shift_step_sends_the_key_with_shift_held() {
    let mut script = Script::new(Box::new([Step::KeyWithShift(Key::Enter)]));
    let mut face = face();
    script.next(&mut face, Time::ZERO);
    assert_eq!(
        face.requests().collect::<Vec<_>>(),
        vec![Request::Key { page: Token::new(7), op: Token::new(7_000_001), key: Key::Enter, shift: true }]
    );
}
const ADDRESS_OP: Token = Token::new(7_000_001);
fn address_step(path: &[u8]) -> (Script, BrowserFace<'static>) {
    let mut script = Script::new(Box::new([Step::Address(path.into())]));
    let mut face = face();
    script.next(&mut face, Time::ZERO);
    assert_eq!(face.requests().collect::<Vec<_>>(), vec![Request::Address { page: Token::new(7), op: ADDRESS_OP }]);
    (script, face)
}
#[test]
fn a_failed_address_step_names_both_addresses() {
    let (_, mut face) = address_step(b"/notes/1");
    face.event(&Event::Address { op: ADDRESS_OP, url: b"http://127.0.0.1:9000/notes/2".as_slice().into() });
    assert!(!face.pending());
    assert_eq!(
        face.failure().as_deref(),
        Some("the page's address was http://127.0.0.1:9000/notes/2, expected the path /notes/1")
    );
}
#[test]
fn an_address_step_passes_on_its_path() {
    let (script, mut face) = address_step(b"/notes/1");
    face.event(&Event::Address { op: ADDRESS_OP, url: b"http://127.0.0.1:9000/notes/1".as_slice().into() });
    assert_eq!((face.failure(), script.at()), (None, 1));
}
#[test]
fn an_address_step_compares_the_query_too() {
    let (_, mut face) = address_step(b"/");
    face.event(&Event::Address { op: ADDRESS_OP, url: b"http://127.0.0.1:9000/?secret=1".as_slice().into() });
    assert_eq!(
        face.failure().as_deref(),
        Some("the page's address was http://127.0.0.1:9000/?secret=1, expected the path /")
    );
}
#[test]
fn an_address_answer_to_another_op_is_ignored() {
    let (_, mut face) = address_step(b"/notes/1");
    face.event(&Event::Address { op: Token::new(7_000_002), url: b"http://127.0.0.1:9000/".as_slice().into() });
    assert_eq!((face.pending(), face.failure()), (true, None));
}
#[test]
fn a_point_press_uses_the_found_figures_node() {
    let mut script = Script::new(Box::new([Step::PressAt {
        find: Find::exactly_named(Role::Drawing, b"Notes by label"),
        at: skein_browser::boundary::Point { x: 128, y: 60 },
    }]));
    let mut face = face();
    script.next(&mut face, Time::ZERO);
    script.next(&mut face, Time::ZERO);
    found(&mut face, Some(seen(b"")));
    script.next(&mut face, Time::ZERO);
    assert_eq!(
        face.requests().collect::<Vec<_>>(),
        vec![Request::Press {
            page: Token::new(7),
            op: Token::new(7_000_002),
            node: 9_000_000_000,
            at: Some(skein_browser::boundary::Point { x: 128, y: 60 }),
        }]
    );
}
const FOCUS: &[(bool, Option<&str>)] = &[
    (true, None),
    (
        false,
        Some(
            "node Find { within: [], target: ExactRole { role: TextField, name: [84, 105, 116, 108, 101] } } is not focused",
        ),
    ),
];
#[test]
fn the_focus_step_checks_the_found_nodes_focus() {
    for &(focused, expected) in FOCUS {
        let mut script = Script::new(Box::new([Step::Focused(Find::exactly_named(Role::TextField, b"Title"))]));
        let mut face = face();
        script.next(&mut face, Time::ZERO);
        script.next(&mut face, Time::ZERO);
        let mut target = seen(b"");
        target.states.focused = focused;
        found(&mut face, Some(target));
        script.next(&mut face, Time::ZERO);
        assert_eq!(face.failure().as_deref(), expected, "focused {focused}");
    }
}
