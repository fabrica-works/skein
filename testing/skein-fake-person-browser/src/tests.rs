use crate::BrowserFace;
use skein_browser::boundary::{Event, Match, Query, Request, Seen, States};
use skein_fake_person::{Doing, Face, Find, Found};
use skein_lib::{List, Token};
use skein_ui::{Act, NodeId, Role};

const FOUND_TARGET: NodeId = NodeId(0); // first node in browser discovery order

fn face() -> BrowserFace<'static> {
    BrowserFace::new(Token::new(7), "127.0.0.1:9000")
}
fn seen(value: &[u8]) -> Seen {
    Seen {
        node: 9_000_000_000,
        role: b"textbox".as_slice().into(),
        name: b"Title".as_slice().into(),
        value: value.into(),
        states: States { focused: false, disabled: false, checked: false, expanded: false, selected: false },
        rect: None,
    }
}
fn found(face: &mut BrowserFace<'_>, seen: Option<Seen>) {
    let Request::Find { op, .. } = face.requests().next().expect("request") else {
        panic!("expected find");
    };
    let mut nodes = List::with_capacity(1);
    if let Some(seen) = seen {
        nodes.push(seen).expect("room");
    }
    face.event(&Event::Found { op, seen: nodes, more: 0 });
}
fn query(face: &mut BrowserFace<'_>) -> Query {
    let Request::Find { query, .. } = face.requests().next().expect("request") else {
        panic!("expected find");
    };
    query
}
#[test]
fn a_find_asks_each_accessible_role_of_the_person_role() {
    let mut face = face();
    face.find(&Find::exactly_named(Role::Image, b"Logo"));
    let requests: Vec<_> = face.requests().collect();
    assert_eq!(
        requests,
        vec![Request::Find {
            page: Token::new(7),
            op: Token::new(7_000_001),
            query: Query {
                role: b"image".as_slice().into(),
                name: b"Logo".as_slice().into(),
                matching: Match::Exact,
                within: None,
                boxes: false
            }
        }]
    );
    face.event(&Event::Found { op: Token::new(7_000_001), seen: List::with_capacity(0), more: 0 });
    assert_eq!(
        query(&mut face),
        Query {
            role: b"StaticText".as_slice().into(),
            name: b"Logo".as_slice().into(),
            matching: Match::Exact,
            within: None,
            boxes: false
        }
    );
}
#[test]
fn the_face_is_pending_until_the_kit_answers() {
    let mut face = face();
    let find = Find::named(Role::TextField, b"Title");
    assert_eq!(face.find(&find), Found::Pending);
    assert_eq!(face.find(&find), Found::Pending);
    found(&mut face, Some(seen(b"")));
    assert_eq!(face.find(&find), Found::Node(FOUND_TARGET));
}
#[test]
fn text_is_found_by_words_within_one_run() {
    let mut face = face();
    face.find(&Find::text(b"No notes"));
    assert_eq!(
        query(&mut face),
        Query {
            role: b"StaticText".as_slice().into(),
            name: b"No notes".as_slice().into(),
            matching: Match::Words,
            within: None,
            boxes: false
        }
    );
}
#[test]
fn a_press_becomes_the_kits_press_at_the_centre() {
    let mut face = face();
    let find = Find::named(Role::Button, b"Title");
    face.find(&find);
    found(&mut face, Some(seen(b"")));
    face.find(&find);
    face.perform(Doing::Act { node: FOUND_TARGET, act: Act::Press }).expect("press");
    assert_eq!(
        face.requests().collect::<Vec<_>>(),
        vec![Request::Press { page: Token::new(7), op: Token::new(7_000_002), node: 9_000_000_000, at: None }]
    );
}
#[test]
fn an_input_into_a_field_holding_words_fails_naming_it() {
    let mut face = face();
    let find = Find::named(Role::TextField, b"Title");
    face.find(&find);
    found(&mut face, Some(seen(b"Words")));
    face.find(&find);
    assert_eq!(
        face.perform(Doing::Act { node: FOUND_TARGET, act: Act::Input { value: b"New".as_slice().into() } }),
        Err("field 'Title' holds words; the kit cannot replace them".into())
    );
}
#[test]
fn a_disabled_node_refuses_acts() {
    let mut face = face();
    let find = Find::named(Role::Button, b"Title");
    face.find(&find);
    let mut node = seen(b"");
    node.states.disabled = true;
    found(&mut face, Some(node));
    assert!(!face.allows(FOUND_TARGET, &Act::Press));
}
#[test]
fn a_target_is_scoped_under_its_ancestor() {
    let mut face = face();
    face.find(
        &Find::named(Role::Button, b"Add").within(Box::new([skein_fake_person::Target::Role {
            role: Role::Region,
            name: b"Notes".as_slice().into(),
        }])),
    );
    found(&mut face, Some(seen(b"")));
    assert_eq!(
        query(&mut face),
        Query {
            role: b"button".as_slice().into(),
            name: b"Add".as_slice().into(),
            matching: Match::Words,
            within: Some(9_000_000_000),
            boxes: false
        }
    );
}
#[test]
fn a_go_uses_the_servers_origin() {
    let mut face = face();
    face.perform(Doing::Go { address: b"/notes/4".as_slice().into() }).expect("go");
    assert_eq!(
        face.requests().collect::<Vec<_>>(),
        vec![Request::Go {
            page: Token::new(7),
            op: Token::new(7_000_001),
            to: skein_browser::boundary::Go::Address(b"http://127.0.0.1:9000/notes/4".as_slice().into())
        }]
    );
}

#[test]
fn the_listening_line_gives_the_address() {
    for (stderr, expected) in [
        (b"starting\nserver at 127.0.0.1:44327\n".as_slice(), Some("127.0.0.1:44327".to_owned())),
        (b"server at 127.0.0.1:44327".as_slice(), None),
        (b"other line\n".as_slice(), None),
    ] {
        assert_eq!(crate::harness::listening_address(stderr, b"server at "), expected);
    }
}

fn warn_go(face: &mut BrowserFace<'_>) {
    face.perform(Doing::Reload).expect("reload accepted");
    let Request::Go { op, .. } = face.requests().next().expect("go") else {
        panic!("expected go");
    };
    face.event(&Event::Warned { op });
    face.event(&Event::Done { op });
}
#[test]
fn the_face_records_a_go_that_meets_its_closing_warning() {
    let mut face = face();
    warn_go(&mut face);
    assert!(face.warned());
}
#[test]
fn a_new_go_clears_the_faces_closing_warning_record() {
    let mut face = face();
    warn_go(&mut face);
    face.perform(Doing::Go { address: b"/notes/7".as_slice().into() }).expect("go accepted");
    assert!(!face.warned());
}
#[test]
fn the_title_is_found_by_the_documents_root_name() {
    let mut face = face();
    face.title(b"Groceries");
    assert_eq!(
        query(&mut face),
        Query {
            role: b"RootWebArea".as_slice().into(),
            name: b"Groceries".as_slice().into(),
            matching: Match::Exact,
            within: None,
            boxes: false
        }
    );
}

fn answer_nodes(face: &mut BrowserFace<'_>, nodes: &[Seen]) {
    let Request::Find { op, .. } = face.requests().next().expect("find") else {
        panic!("expected find");
    };
    let mut seen = List::with_capacity(u32::try_from(nodes.len()).unwrap());
    for node in nodes {
        seen.push(node.clone()).unwrap();
    }
    face.event(&Event::Found { op, seen, more: 0 });
}
fn choosing(face: &mut BrowserFace<'_>, selected: usize, target: usize) {
    let mut options = vec![seen(b""), seen(b""), seen(b"")];
    for (index, node) in options.iter_mut().enumerate() {
        node.node = 100 + u64::try_from(index).unwrap();
        node.states.selected = index == selected;
    }
    face.find(&Find::exactly_named(Role::Option, b"task"));
    answer_nodes(face, &options[target..=target]);
    face.find(&Find::exactly_named(Role::Option, b"task"));
    face.perform(Doing::Act { node: FOUND_TARGET, act: Act::Press }).unwrap();
    answer_nodes(face, &[seen(b"")]);
    answer_nodes(face, &options);
}
#[test]
fn choosing_an_option_presses_its_select() {
    let mut face = face();
    choosing(&mut face, 0, 1);
    assert_eq!(
        face.requests().collect::<Vec<_>>(),
        vec![Request::Press { page: Token::new(7), op: Token::new(7_000_005), node: 9_000_000_000, at: None }]
    );
}
const OPTION_KEYS: &[(usize, usize, &[skein_browser::boundary::Key])] = &[
    (
        0,
        2,
        &[
            skein_browser::boundary::Key::ArrowDown,
            skein_browser::boundary::Key::ArrowDown,
            skein_browser::boundary::Key::Enter,
        ],
    ),
    (
        2,
        0,
        &[
            skein_browser::boundary::Key::ArrowUp,
            skein_browser::boundary::Key::ArrowUp,
            skein_browser::boundary::Key::Enter,
        ],
    ),
    (1, 1, &[skein_browser::boundary::Key::Enter]),
];
#[test]
fn choosing_an_option_uses_keys_from_the_current_selection() {
    for &(selected, target, expected) in OPTION_KEYS {
        let mut face = face();
        choosing(&mut face, selected, target);
        let Request::Press { mut op, .. } = face.requests().next().expect("press select") else {
            panic!("expected press");
        };
        let mut keys = Vec::new();
        while face.pending() {
            face.event(&Event::Done { op });
            if let Some(Request::Key { op: next, key, .. }) = face.requests().next() {
                op = next;
                keys.push(key);
            }
        }
        assert_eq!(keys, expected, "selected {selected}, target {target}");
    }
}

type NativeKeys = (bool, &'static [u8], Option<&'static [u8]>);
const NATIVE_KEYS: &[NativeKeys] = &[
    (false, b"2026-10-15", Some(b"10152026")),
    (false, b"0001-12-31", Some(b"12310001")),
    (false, b"0000-99-99", Some(b"99990000")),
    (false, b"", Some(b"")),
    (false, b"2026-1-5", None),
    (false, b"2026/10/15", None),
    (false, b"yyyy-mm-dd", None),
    (true, b"00:00", Some(b"1200A")),
    (true, b"09:05", Some(b"0905A")),
    (true, b"12:00", Some(b"1200P")),
    (true, b"14:30", Some(b"0230P")),
    (true, b"23:59", Some(b"1159P")),
    (true, b"", Some(b"")),
    (true, b"14:30:00", None),
    (true, b"24:00", None),
    (true, b"09:60", None),
    (true, b"9:05", None),
    (true, b"hh:mm", None),
];
#[test]
fn a_date_and_a_time_become_the_keys_of_their_parts() {
    for &(time, value, expected) in NATIVE_KEYS {
        let actual = if time { crate::keys::time(value) } else { crate::keys::date(value) };
        assert_eq!(actual.as_deref(), expected, "time={time}, value={value:?}");
    }
}
fn native_field(face: &mut BrowserFace<'_>, role: &[u8], value: &[u8]) {
    let find = Find::exactly_named(Role::TextField, b"Due date");
    face.find(&find);
    let mut node = seen(value);
    node.role = role.into();
    node.name = b"Due date".as_slice().into();
    found(face, Some(node));
    face.find(&find);
}
#[test]
fn a_date_field_is_typed_by_the_kits_keys() {
    let mut face = face();
    native_field(&mut face, b"Date", b"");
    face.perform(Doing::Act { node: FOUND_TARGET, act: Act::Input { value: b"2026-10-15".as_slice().into() } })
        .unwrap();
    assert_eq!(
        face.requests().collect::<Vec<_>>(),
        vec![Request::Keys {
            page: Token::new(7),
            op: Token::new(7_000_002),
            node: 9_000_000_000,
            text: b"10152026".as_slice().into(),
        }]
    );
}
#[test]
fn a_time_field_is_typed_by_the_kits_keys() {
    let mut face = face();
    native_field(&mut face, b"InputTime", b"");
    face.perform(Doing::Act { node: FOUND_TARGET, act: Act::Input { value: b"14:30".as_slice().into() } }).unwrap();
    assert_eq!(
        face.requests().collect::<Vec<_>>(),
        vec![Request::Keys {
            page: Token::new(7),
            op: Token::new(7_000_002),
            node: 9_000_000_000,
            text: b"0230P".as_slice().into(),
        }]
    );
}
#[test]
fn an_invalid_native_field_value_fails_naming_the_field_and_value() {
    let mut face = face();
    native_field(&mut face, b"Date", b"");
    assert_eq!(
        face.perform(Doing::Act { node: FOUND_TARGET, act: Act::Input { value: b"2026-1-5".as_slice().into() } }),
        Err("field 'Due date' takes no value '2026-1-5'".into())
    );
}
#[test]
fn native_keys_still_refuse_a_field_holding_words_first() {
    let mut face = face();
    native_field(&mut face, b"Date", b"2026-10-15");
    assert_eq!(
        face.perform(Doing::Act { node: FOUND_TARGET, act: Act::Input { value: b"invalid".as_slice().into() } }),
        Err("field 'Due date' holds words; the kit cannot replace them".into())
    );
}
