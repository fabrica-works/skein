use crate::Binding;
use crate::Limits;
use crate::worst_case;
use crate::{Builder, DrawingKind, Key, Owner, Role, Run, State, Style};

use skein_lib::{List, Token};

use super::fixtures::LIMITS;

#[test]
fn nodes_are_preorder_and_sizes_include_descendants() {
    let mut builder = Builder::new(&LIMITS);
    builder.open(Role::Page);
    builder.open(Role::List);
    builder.open(Role::Item);
    builder.key(Key { owner: Owner { number: 65_536 }, kind: 1, a: 7, b: 0 });
    builder.text(Run::Plain, b"one");
    builder.close();
    builder.open(Role::Item);
    builder.close();
    builder.close();
    builder.close();
    let tree = builder.finish();
    let mut roles = List::with_capacity(5);
    for node in tree.nodes() {
        roles.push(node.role).expect("five nodes");
    }
    assert_eq!(roles.as_slice(), [Role::Page, Role::List, Role::Item, Role::Text, Role::Item]);
    let mut sizes = List::with_capacity(5);
    for node in tree.nodes() {
        sizes.push(node.size).expect("five nodes");
    }
    assert_eq!(sizes.as_slice(), [5, 4, 2, 1, 1]);
}

#[test]
fn a_fields_own_write_count_and_semantic_attributes_are_kept() {
    let mut builder = Builder::new(&LIMITS);
    builder.open(Role::TextField);
    builder.name(b"Amount");
    builder.value(b"42", 19);
    builder.state(State::Invalid);
    builder.state(State::Invalid);
    builder.style(Style { owner: Owner { number: 3 }, token: 2 });
    let binding = Binding { owner: Owner { number: 8 }, token: Token::new(5) };
    builder.bind(binding);
    let shortcut = crate::Shortcut { key: b'a', modifier: crate::Modifier::Primary };
    builder.shortcut(shortcut);
    builder.close();
    let tree = builder.finish();
    let node = tree.get(0).expect("field exists");
    let value = node.value.as_ref().expect("value exists");
    assert_eq!(value.written, 19);
    assert_eq!(value.text.as_ref(), b"42");
    assert_eq!(node.states.as_slice(), [State::Invalid]);
    assert_eq!(node.binding, Some(binding));
    assert_eq!(node.shortcut, Some(shortcut));
}

#[test]
fn a_drawing_keeps_opaque_data_and_readable_fallback() {
    let mut builder = Builder::new(&LIMITS);
    let kind = DrawingKind { owner: Owner { number: 2 }, kind: 1 };
    builder.drawing(kind, b"Activity", b"1234", 4);
    builder.text(Run::Strong, b"four changes");
    builder.close();
    let tree = builder.finish();
    let node = tree.get(0).expect("drawing exists");
    assert_eq!(node.size, 2);
    assert_eq!(node.name.as_deref(), Some(b"Activity".as_slice()));
    assert_eq!(node.drawing.as_ref().expect("drawing data").data.as_ref(), b"1234");
}

#[test]
#[should_panic(expected = "drawing fits its owner's byte bound")]
fn a_drawing_past_its_owners_bound_asserts() {
    let mut builder = Builder::new(&LIMITS);
    builder.drawing(DrawingKind { owner: Owner { number: 65_536 }, kind: 1 }, b"name", b"12345", 4);
}

#[test]
#[should_panic(expected = "drawing fits its owner's byte bound")]
fn a_drawing_past_the_trees_bound_asserts() {
    let mut builder = Builder::new(&LIMITS);
    builder.drawing(DrawingKind { owner: Owner { number: 65_536 }, kind: 1 }, b"name", b"123456789", 16);
}

#[test]
#[should_panic(expected = "a drawing is shown and never bound")]
fn a_drawing_cannot_be_bound() {
    let mut builder = Builder::new(&LIMITS);
    builder.drawing(DrawingKind { owner: Owner { number: 65_536 }, kind: 1 }, b"name", b"", 0);
    builder.bind(Binding { owner: Owner { number: 65_536 }, token: Token::new(1) });
}

const BINDING_ROLES: &[(Role, bool)] = &[
    (Role::Page, false),
    (Role::Banner, false),
    (Role::Navigation, false),
    (Role::Main, false),
    (Role::Complementary, false),
    (Role::ContentInfo, false),
    (Role::Form, false),
    (Role::Search, false),
    (Role::Region, false),
    (Role::Heading { level: 1 }, false),
    (Role::Paragraph, false),
    (Role::Text, false),
    (Role::List, false),
    (Role::Item, false),
    (Role::Table, false),
    (Role::Row, false),
    (Role::Cell, false),
    (Role::Term, false),
    (Role::Description, false),
    (Role::Button, true),
    (Role::Link, true),
    (Role::TextField, true),
    (Role::TextArea, true),
    (Role::Choice, true),
    (Role::ChoiceGroup, false),
    (Role::Check, true),
    (Role::Dialog, false),
    (Role::Status, false),
    (Role::Alert, false),
    (Role::Log, false),
    (Role::Tree, false),
    (Role::TreeItem, false),
    (Role::Tabs, false),
    (Role::Tab, true),
    (Role::Progress, false),
    (Role::Time, false),
    (Role::Code, false),
    (Role::Quote, false),
    (Role::Separator, false),
    (Role::Menu, false),
    (Role::MenuItem, true),
    (Role::Disclosure, false),
    (Role::Image, false),
    (Role::Figure, false),
    (Role::Slot, false),
    (Role::Drawing, false),
];

#[test]
fn bind_takes_only_the_roles_a_person_acts_on() {
    for &(role, allowed) in BINDING_ROLES {
        let outcome = std::panic::catch_unwind(|| {
            let mut builder = Builder::new(&LIMITS);
            builder.open(role);
            builder.bind(Binding { owner: Owner { number: 1 }, token: Token::new(1) });
        });
        assert_eq!(outcome.is_ok(), allowed, "{role:?}");
    }
}

#[test]
#[should_panic(expected = "tree fits its node capacity")]
fn building_past_node_capacity_asserts() {
    let limits = Limits { nodes: 1, ..LIMITS };
    let mut builder = Builder::new(&limits);
    builder.open(Role::Page);
    builder.text(Run::Plain, b"child");
}

#[test]
#[should_panic(expected = "tree fits its depth capacity")]
fn building_past_depth_capacity_asserts() {
    let limits = Limits { depth: 1, ..LIMITS };
    let mut builder = Builder::new(&limits);
    builder.open(Role::Page);
    builder.open(Role::Region);
}

#[test]
#[should_panic(expected = "node fits its text capacity")]
fn name_and_value_share_a_nodes_byte_bound() {
    let limits = Limits { text_per_node: 5, ..LIMITS };
    let mut builder = Builder::new(&limits);
    builder.open(Role::TextField);
    builder.name(b"name");
    builder.value(b"12", 1);
}

#[test]
#[should_panic(expected = "tree fits its text capacity")]
fn all_text_shares_the_trees_byte_bound() {
    let limits = Limits { text_bytes: 5, ..LIMITS };
    let mut builder = Builder::new(&limits);
    builder.text(Run::Plain, b"one");
    builder.text(Run::Plain, b"two");
}

#[test]
fn spare_node_storage_is_reused_and_replaced_bytes_are_recounted() {
    let limits = Limits { text_bytes: 5, ..LIMITS };
    let mut builder = Builder::new(&limits);
    builder.open(Role::TextArea);
    builder.value(b"long", 1);
    builder.value(b"small", 2);
    builder.close();
    let tree = builder.finish();
    let storage = tree.nodes().as_ptr();
    let mut builder = Builder::reuse(tree, &limits);
    builder.text(Run::Plain, b"again");
    let tree = builder.finish();
    assert_eq!(tree.nodes().as_ptr(), storage);
    assert_eq!(tree.len(), 1);
    assert!(worst_case(&limits).expect("small limits fit") > 0);
}

#[test]
fn runs_and_link_addresses_keep_their_owner_tokens() {
    let mut builder = Builder::new(&LIMITS);
    builder.open(Role::Link);
    let link = Binding { owner: Owner { number: 1 }, token: Token::new(77) };
    builder.link(link);
    builder.open(Role::Text);
    builder.run(Run::Plain, b"read ");
    builder.run(Run::Emphasis, b"more");
    builder.close();
    builder.close();
    let tree = builder.finish();
    assert_eq!(tree.get(0).expect("link").link, Some(link));
    assert_eq!(tree.get(1).expect("text").text.len(), 2);
}

#[test]
fn a_built_text_leaf_reserves_one_run() {
    let mut builder = Builder::new(&LIMITS);
    builder.text(Run::Plain, b"one");
    let tree = builder.finish();
    let node = tree.get(0).expect("text node exists");
    assert_eq!(node.text.capacity(), 1);
}

#[test]
#[should_panic(expected = "shortcut keys are letters or digits")]
fn a_shortcut_on_a_key_outside_letters_and_digits_is_refused() {
    let mut builder = Builder::new(&LIMITS);
    builder.open(Role::Button);
    builder.bind(Binding { owner: Owner { number: 65_536 }, token: Token::new(1) });
    builder.shortcut(crate::Shortcut { key: b'!', modifier: crate::Modifier::Plain });
}

#[test]
fn an_image_keeps_its_owners_picture() {
    let mut builder = Builder::new(&LIMITS);
    builder.open(Role::Image);
    builder.name(b"A mark");
    let picture = crate::Picture { owner: Owner { number: 3 }, number: 7 };
    builder.picture(picture);
    builder.close();
    let tree = builder.finish();
    assert_eq!(tree.get(0).unwrap().picture, Some(picture));
    assert_eq!(crate::Made::of(tree.get(0).unwrap()).attribute.picture, Some(picture));
}
