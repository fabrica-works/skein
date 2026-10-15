use super::fixtures;
use crate::{Command, Print, Refusal, command, refusal_line, render};
use skein_lib::{Queue, bytes::copy_of};
use skein_ui::Act;
use skein_ui::{Builder, NodeId, Role, State, Tree};
fn page() -> Tree {
    fixtures::page(|b| fixtures::button(b, b"Accept", true))
}
fn numbers(tree: &Tree) -> Print {
    let mut print = Print::new(&fixtures::LIMITS).unwrap();
    render(tree, b"ops", None, &mut print);
    print
}
fn field(b: &mut Builder, submit: bool) {
    b.open(Role::TextArea);
    b.name(b"Message");
    b.bind(fixtures::binding());
    if submit {
        b.state(State::SubmitsOnEnter);
    }
    b.close();
}
#[test]
fn a_number_presses_its_button() {
    let tree = page();
    let print = numbers(&tree);
    assert_eq!(
        command(b"  1  ", Some(print.numbers.as_slice()), &tree, 10),
        Command::Act { node: tree.get(3).unwrap().id, first: Act::Press, submit: false }
    );
}
#[test]
fn a_number_toggles_its_check() {
    let tree = fixtures::page(|b| {
        b.open(Role::Check);
        b.name(b"Option");
        b.bind(fixtures::binding());
        b.close();
        b.open(Role::Choice);
        b.name(b"Choice");
        b.bind(fixtures::binding());
        b.close();
    });
    let print = numbers(&tree);
    for (line, node) in [(b"1", tree.get(3).unwrap().id), (b"2", tree.get(4).unwrap().id)] {
        assert_eq!(
            command(line, Some(print.numbers.as_slice()), &tree, 10),
            Command::Act { node, first: Act::Toggle, submit: false }
        );
    }
}
#[test]
fn words_after_a_number_fill_and_submit_its_field() {
    let tree = fixtures::page(|b| field(b, true));
    let print = numbers(&tree);
    for (line, value) in [(b" 1   hello  world  ".as_slice(), b"hello  world".as_slice()), (b"1", b"")] {
        assert_eq!(
            command(line, Some(print.numbers.as_slice()), &tree, 40),
            Command::Act { node: tree.get(3).unwrap().id, first: Act::Input { value: copy_of(value) }, submit: true }
        );
    }
}
#[test]
fn words_for_a_field_that_does_not_submit_only_fill_it() {
    let tree = fixtures::page(|b| field(b, false));
    let print = numbers(&tree);
    assert_eq!(
        command(b"1 hello", Some(print.numbers.as_slice()), &tree, 20),
        Command::Act { node: tree.get(3).unwrap().id, first: Act::Input { value: copy_of(b"hello") }, submit: false }
    );
}
#[test]
fn p_prints_and_q_closes() {
    let tree = page();
    for (line, wanted) in [(b"p", Command::Print), (b"q", Command::Close)] {
        assert_eq!(command(line, None, &tree, 1), wanted);
    }
}
#[test]
fn an_empty_line_does_nothing() {
    assert_eq!(command(b"    ", None, &page(), 0), Command::Nothing);
}
#[test]
fn a_number_never_printed_is_refused() {
    let tree = page();
    let print = numbers(&tree);
    for (line, number) in [(b"0".as_slice(), 0), (b"2", 2), (b"18446744073709551615", u64::MAX)] {
        assert_eq!(
            command(line, Some(print.numbers.as_slice()), &tree, 30),
            Command::Refused { refusal: Refusal::NoNumber, number }
        );
    }
    assert_eq!(refusal_line(b"ops", Refusal::NoNumber, 2).as_ref(), b"ops: no [2] on the page\n");
}
#[test]
fn a_number_whose_node_left_is_gone() {
    let tree = page();
    let print = numbers(&tree);
    let disabled = fixtures::page(|b| fixtures::button(b, b"Accept", false));
    let unbound = fixtures::page(|b| {
        b.open(Role::Button);
        b.name(b"Accept");
        b.close();
    });
    for changed in [fixtures::page(|_| {}), disabled, unbound] {
        assert_eq!(
            command(b"1", Some(print.numbers.as_slice()), &changed, 1),
            Command::Refused { refusal: Refusal::Gone, number: 1 }
        );
    }
    assert_eq!(refusal_line(b"ops", Refusal::Gone, 1).as_ref(), b"ops: [1] is gone\n");
}
#[test]
fn words_for_a_button_are_refused() {
    let tree = page();
    let print = numbers(&tree);
    assert_eq!(
        command(b"1 hello", Some(print.numbers.as_slice()), &tree, 20),
        Command::Refused { refusal: Refusal::TakesNoWords, number: 1 }
    );
    assert_eq!(refusal_line(b"ops", Refusal::TakesNoWords, 1).as_ref(), b"ops: [1] takes no words\n");
}
#[test]
fn a_number_before_the_first_print_is_refused() {
    assert_eq!(command(b"1", None, &page(), 1), Command::Refused { refusal: Refusal::NoPage, number: 1 });
    assert_eq!(refusal_line(b"ops", Refusal::NoPage, 0).as_ref(), b"ops: no page yet\n");
}
#[test]
fn a_line_past_the_longest_is_refused() {
    assert_eq!(command(b"1 hello", None, &page(), 3), Command::Refused { refusal: Refusal::TooLong, number: 0 });
    assert_eq!(refusal_line(b"ops", Refusal::TooLong, 0).as_ref(), b"ops: line too long\n");
}
#[test]
fn anything_else_names_the_commands() {
    for line in [b"hello".as_slice(), b"p words", b"-1", b"1x", b"18446744073709551616"] {
        assert_eq!(command(line, None, &page(), 30), Command::Refused { refusal: Refusal::Unknown, number: 0 });
    }
    assert_eq!(refusal_line(b"ops", Refusal::Unknown, 0).as_ref(), b"ops: commands: n, n words, p, q\n");
}
#[test]
fn numbers_hold_across_a_print_and_renumber_at_the_next() {
    let old = fixtures::page(|b| {
        fixtures::button(b, b"Accept", true);
        fixtures::button(b, b"Release", true);
    });
    let mut print = numbers(&old);
    let mut new = fixtures::page(|b| {
        fixtures::button(b, b"Accept", false);
        fixtures::button(b, b"Release", true);
    });
    let mut patches = Queue::with_capacity(fixtures::LIMITS.patches);
    skein_ui::diff(&old, &mut new, &fixtures::LIMITS, &mut 50, &mut patches);
    let release = new.get(4).unwrap().id;
    assert_eq!(release, old.get(4).unwrap().id);
    assert_eq!(
        command(b"2", Some(print.numbers.as_slice()), &new, 1),
        Command::Act { node: release, first: Act::Press, submit: false }
    );
    render(&new, b"ops", None, &mut print);
    assert_eq!(print.numbers.as_slice(), [release]);
    assert_eq!(
        command(b"1", Some(print.numbers.as_slice()), &new, 1),
        Command::Act { node: release, first: Act::Press, submit: false }
    );
    assert_eq!(
        command(b"2", Some(print.numbers.as_slice()), &new, 1),
        Command::Refused { refusal: Refusal::NoNumber, number: 2 }
    );
    assert_ne!(release, NodeId(0));
}
