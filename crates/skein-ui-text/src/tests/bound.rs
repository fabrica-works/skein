use super::fixtures;
use crate::{Print, render, worst_case};
use skein_lib::Queue;
use skein_ui::{Builder, EntryKind, Limits, Role, Run, Spans, State, Tree};

const LIMITS: Limits = Limits { nodes: 16, depth: 5, text_per_node: 16, text_bytes: 256, ..fixtures::LIMITS };
const ROWS: &[(&str, u8)] = &[("newlines", b'\n'), ("control bytes", 0)];

fn full_node(b: &mut Builder, role: Role, byte: u8) {
    b.open(role);
    b.description(&[byte; 8]);
    if role == Role::TextField {
        b.name(&[byte; 4]);
        b.value(&[byte; 4], u64::MAX);
        b.entry(EntryKind::Date);
        b.bind(fixtures::binding());
        b.state(State::Required);
        b.state(State::SubmitsOnEnter);
        b.focus(u64::MAX);
    } else if role == Role::Text {
        b.name(&[byte; 4]);
        b.run(Run::Code, &[byte; 4]);
    } else {
        b.name(&[byte; 8]);
    }
    for state in
        [State::Disabled, State::Expanded, State::Busy, State::Current, State::Invalid, State::Pressed, State::Selected]
    {
        b.state(state);
    }
    if [Role::List, Role::Table, Role::Tree].contains(&role) {
        b.count(u32::MAX);
    }
    if [Role::Item, Role::Row, Role::TreeItem].contains(&role) {
        b.position(u32::MAX);
    }
    if role == Role::ColumnHeader {
        b.sort(skein_ui::Sort::Descending);
    }
    if role == Role::Cell || role == Role::ColumnHeader {
        b.spans(Spans { rows: u16::MAX, columns: u16::MAX });
    }
    if role == Role::Select {
        b.state(State::Required);
    }
    if role == Role::Option {
        b.bind(fixtures::binding());
    }
}
fn full_page(byte: u8) -> Tree {
    let mut b = Builder::new(&LIMITS);
    full_node(&mut b, Role::Page, byte);
    full_node(&mut b, Role::Main, byte);
    full_node(&mut b, Role::List, byte);
    full_node(&mut b, Role::Item, byte);
    full_node(&mut b, Role::Quote, byte);
    b.close();
    b.close();
    full_node(&mut b, Role::TextField, byte);
    b.close();
    b.close();
    full_node(&mut b, Role::Table, byte);
    full_node(&mut b, Role::Row, byte);
    for role in [Role::ColumnHeader, Role::Cell] {
        full_node(&mut b, role, byte);
        b.close();
    }
    b.close();
    b.close();
    full_node(&mut b, Role::Tree, byte);
    full_node(&mut b, Role::TreeItem, byte);
    b.close();
    b.close();
    full_node(&mut b, Role::Select, byte);
    full_node(&mut b, Role::Option, byte);
    b.close();
    b.close();
    for role in [Role::Text, Role::Code] {
        full_node(&mut b, role, byte);
        b.close();
    }
    b.close();
    b.close();
    let mut tree = b.finish();
    let mut patches = Queue::with_capacity(LIMITS.patches);
    skein_ui::diff(&Tree::new(LIMITS.nodes), &mut tree, &LIMITS, &mut 0, &mut patches);
    tree
}

#[test]
fn the_print_of_a_full_page_fits_its_worst_case() {
    let (lines, bytes) = worst_case(&LIMITS).unwrap();
    for &(name, byte) in ROWS {
        let tree = full_page(byte);
        let mut print = Print::new(&LIMITS).unwrap();
        render(&tree, &[byte; 16], None, &mut print);
        let mut actual_lines = 0;
        for &printed in print.text.as_slice() {
            if printed == b'\n' {
                actual_lines += 1;
            }
        }
        assert!(actual_lines <= lines, "{name}: {actual_lines} > {lines}");
        assert!(u64::from(print.text.len()) <= bytes, "{name}: {} > {bytes}", print.text.len());
        assert!(print.text.len() <= print.text.capacity(), "{name}: The tree fits its own room");
        assert!(u64::from(print.text.capacity()) <= bytes, "{name}: The tree's room fits its worst case");
    }
}

#[test]
fn a_small_print_takes_its_room_from_its_tree() {
    let large = Limits { depth: 100_000, text_bytes: 100_000, ..fixtures::LIMITS };
    assert!(worst_case(&large).unwrap().1 > u64::from(u32::MAX));
    let tree = fixtures::page(|b| fixtures::button(b, b"Accept", true));
    let mut small = Print::new(&fixtures::LIMITS).unwrap();
    let mut print = Print::new(&large).unwrap();
    assert_eq!(print.text.capacity(), 0);
    render(&tree, b"test", None, &mut small);
    render(&tree, b"test", None, &mut print);
    assert_eq!(print.text.as_slice(), b"== test == \n[1] Accept\n");
    assert_eq!(print.text.capacity(), small.text.capacity());
    assert_eq!(print.numbers.as_slice(), small.numbers.as_slice());
}

#[test]
fn a_page_past_a_u32_prints_no_numbers() {
    let limits = Limits { nodes: 33_000, depth: 33_000, text_bytes: 4, text_per_node: 4, ..fixtures::LIMITS };
    let mut builder = Builder::new(&limits);
    for _ in 0_u32..33_000 {
        builder.open(Role::Page);
    }
    for _ in 0_u32..33_000 {
        builder.close();
    }
    let tree = builder.finish();
    let mut print = Print::new(&limits).unwrap();
    let small = fixtures::page(|b| fixtures::button(b, b"Go", true));
    render(&small, b"test", None, &mut print);
    assert_eq!(print.numbers.len(), 1);
    render(&tree, b"test", None, &mut print);
    assert_eq!(print.text.as_slice(), b"test: page too long to print\n");
    assert!(print.numbers.is_empty());
    assert_eq!(
        crate::command(b"1", Some(print.numbers.as_slice()), &tree, 32),
        crate::Command::Refused { refusal: crate::Refusal::NoNumber, number: 1 }
    );
}
