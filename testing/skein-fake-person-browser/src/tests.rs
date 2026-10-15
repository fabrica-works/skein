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
