use crate::{Print, render};
use skein_ui::{Role, Run};
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
