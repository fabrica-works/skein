use crate::{Print, render};
use skein_ui::{Role, Run};
mod bound;
mod command;
mod fixtures;

#[test]
#[expect(clippy::disallowed_macros, reason = "hand-written golden bytes fix the renderer's contract")]
fn a_tree_with_every_role_and_state_prints_its_golden_text() {
    let tree = fixtures::page(fixtures::roles);
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    assert_eq!(print.text.as_slice(), include_bytes!("tests/golden/roles.txt"));
    assert_eq!(print.numbers.len(), 14);
}
#[test]
#[expect(clippy::disallowed_macros, reason = "hand-written golden bytes cover unsafe terminal text")]
fn control_bytes_and_bad_utf8_print_as_replacement() {
    let tree = fixtures::page(|b| {
        b.open(Role::Region);
        b.name(b"Indented");
        b.text(Run::Plain, b"a\x00\x1b\x1f\x7f\xc2\x80\xc2\x9f\tend\nnext\xff\xc0\x80\xe2\x82\xac");
        b.close();
    });
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    assert_eq!(print.text.as_slice(), include_bytes!("tests/golden/bytes.txt"));
}
#[test]
fn a_name_ending_in_its_own_mark_takes_no_colon() {
    let tree = fixtures::page(|b| {
        for name in [b"What shall we change?".as_slice(), b"Stop!", b"Note:", b"Plain"] {
            b.open(Role::Region);
            b.name(name);
            fixtures::leaf(b, Role::Paragraph, b"in");
            b.close();
        }
    });
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    assert_eq!(
        print.text.as_slice(),
        b"== ops == \nWhat shall we change?\n  in\nStop!\n  in\nNote:\n  in\nPlain:\n  in\n".as_slice()
    );
}
#[test]
fn a_disabled_button_prints_a_dash_not_a_number() {
    let tree = fixtures::page(|b| {
        fixtures::button(b, b"Accept", true);
        fixtures::button(b, b"Release", false);
    });
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    assert_eq!(print.text.as_slice(), b"== ops == \n[1] Accept\n[-] Release (disabled)\n");
    assert_eq!(print.numbers.as_slice(), [tree.get(3).unwrap().id]);
}
#[test]
fn the_first_line_is_the_main_heading_and_the_link() {
    let tree = fixtures::page(|b| {
        b.open(Role::Region);
        b.region(fixtures::STATUS);
        fixtures::leaf(b, Role::Status, b"Connected");
        fixtures::leaf(b, Role::Status, b"Behind");
        b.close();
        fixtures::leaf(b, Role::Heading { level: 1 }, b"Inbox");
        fixtures::leaf(b, Role::Heading { level: 1 }, b"Again");
    });
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", Some(fixtures::STATUS), &mut print);
    assert_eq!(print.text.as_slice(), b"== Inbox == Connected; Behind\n# Again\n");
}

#[test]
fn no_status_region_ends_the_first_line_after_its_space() {
    let tree = fixtures::page(|b| {
        b.open(Role::Region);
        b.region(fixtures::STATUS);
        fixtures::leaf(b, Role::Status, b"Connected");
        b.close();
        fixtures::leaf(b, Role::Heading { level: 1 }, b"Inbox");
    });
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    assert_eq!(print.text.as_slice(), b"== Inbox == \n* Connected\n");
}

#[test]
fn the_properties_print_their_words() {
    let tree = fixtures::page(fixtures::properties);
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    let expected = std::fs::read("src/tests/golden/properties.txt").expect("hand-written property words");
    assert_eq!(print.text.as_slice(), expected.as_slice());
}

#[test]
fn a_secret_never_prints_its_bytes() {
    let tree = fixtures::page(|b| {
        b.open(Role::TextField);
        b.name(b"Password");
        b.entry(skein_ui::EntryKind::Secret);
        b.value(b"private-token", 1);
        b.bind(fixtures::binding());
        b.close();
    });
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    assert_eq!(print.text.as_slice(), b"== ops == \n[1] Password: (hidden)\n");
}

#[test]
fn the_first_line_is_followed_by_its_descriptions() {
    let tree = fixtures::page(|b| {
        b.open(Role::Heading { level: 1 });
        b.description(b"Heading description");
        b.open(Role::Text);
        b.text(Run::Plain, b"Inbox");
        b.description(b"Inline description");
        b.close();
        b.close();
    });
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    assert_eq!(print.text.as_slice(), b"== Inbox == \n  Heading description\n  Inline description\n");
}

#[test]
fn every_role_state_and_property_prints() {
    let tree = fixtures::page_with_limits(fixtures::every_node, &fixtures::EVERY_LIMITS);
    let mut print = Print::new(&fixtures::EVERY_LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    let expected = std::fs::read("src/tests/golden/every.txt").expect("hand-written complete tree");
    assert_eq!(print.text.as_slice(), expected.as_slice());
}

#[test]
fn the_first_line_without_its_status_region() {
    let tree = fixtures::page(fixtures::first_lines);
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", Some(fixtures::STATUS), &mut print);
    let expected = std::fs::read("src/tests/golden/first-line.txt").expect("hand-written absent status region");
    assert_eq!(print.text.as_slice(), expected.as_slice());
}

#[test]
fn control_bytes_print_as_replacements_everywhere() {
    let tree = fixtures::page(fixtures::control_bytes);
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(&tree, b"ops", None, &mut print);
    let expected = std::fs::read("src/tests/golden/bytes-everywhere.txt").expect("hand-written replacement characters");
    assert_eq!(print.text.as_slice(), expected.as_slice());
}
