use alloc::boxed::Box;
use skein_lib::{Queue, Token};

use crate::tests::fixtures::LIMITS;
use crate::{Act, Binding, BoundAct, Builder, DrawingKind, Limits, NodeId, Owner, Role, State, View};

const BINDING: Binding = Binding { owner: Owner { number: 2 }, token: Token::new(7) };
const BUTTON: NodeId = NodeId(2); // page, button
const LINK: NodeId = NodeId(3); // page, button, link
const TAB: NodeId = NodeId(4); // page, button, link, tab
const MENU_ITEM: NodeId = NodeId(5); // page, button, link, tab, menu item
const OPTION: NodeId = NodeId(6); // page, button, link, tab, menu item, option
const CHECK: NodeId = NodeId(7); // page, button, link, tab, menu item, option, check
const CHOICE: NodeId = NodeId(8); // page, button, link, tab, menu item, option, check, choice
const DRAWING: NodeId = NodeId(9); // page, button, link, tab, menu item, option, check, choice, drawing
const FIELD: NodeId = NodeId(10); // page, button, link, tab, menu item, option, check, choice, drawing, field
const SUBMIT_FIELD: NodeId = NodeId(11); // page through field, submitting field
const AREA: NodeId = NodeId(12); // page through submitting field, area
const SUBMIT_AREA: NodeId = NodeId(13); // page through area, submitting area

fn finish(view: &mut View, builder: Builder) {
    view.finish(builder, 1, &mut Queue::with_capacity(LIMITS.patches));
}

fn page() -> View {
    let mut view = View::new(&Limits { nodes: 16, bindings: 12, ..LIMITS });
    let mut builder = view.builder();
    builder.open(Role::Page);
    for (role, submits) in [
        (Role::Button, false),
        (Role::Link, false),
        (Role::Tab, false),
        (Role::MenuItem, false),
        (Role::Option, false),
        (Role::Check, false),
        (Role::Choice, false),
        (Role::Drawing, false),
        (Role::TextField, false),
        (Role::TextField, true),
        (Role::TextArea, false),
        (Role::TextArea, true),
    ] {
        builder.open(role);
        builder.bind(BINDING);
        if submits {
            builder.state(State::SubmitsOnEnter);
        }
        builder.close();
    }
    builder.close();
    finish(&mut view, builder);
    view
}

fn acts() -> [(&'static str, Act); 5] {
    [
        ("press", Act::Press),
        ("input", Act::Input { value: Box::from(*b"42") }),
        ("submit", Act::Submit),
        ("toggle", Act::Toggle),
        ("drawing", Act::Drawing { data: Box::from(*b"12345678") }),
    ]
}

const ROLE_ACTS: &[(NodeId, [Option<Binding>; 5])] = &[
    (BUTTON, [Some(BINDING), None, None, None, None]),
    (LINK, [Some(BINDING), None, None, None, None]),
    (TAB, [Some(BINDING), None, None, None, None]),
    (MENU_ITEM, [Some(BINDING), None, None, None, None]),
    (OPTION, [Some(BINDING), None, None, None, None]),
    (CHECK, [None, None, None, Some(BINDING), None]),
    (CHOICE, [None, None, None, Some(BINDING), None]),
    (DRAWING, [None, None, None, None, Some(BINDING)]),
    (FIELD, [None, Some(BINDING), None, None, None]),
    (SUBMIT_FIELD, [None, Some(BINDING), Some(BINDING), None, None]),
    (AREA, [None, Some(BINDING), None, None, None]),
    (SUBMIT_AREA, [None, Some(BINDING), Some(BINDING), None, None]),
];

#[test]
fn each_role_takes_only_its_acts() {
    let view = page();
    for &(node, expected) in ROLE_ACTS {
        for ((name, act), expected) in acts().into_iter().zip(expected) {
            let result = view.act(node, act);
            match &result {
                Some(bound) => assert_eq!(Some(bound.binding), expected, "{node:?}, {name}: {result:?}"),
                None => assert_eq!(None, expected, "{node:?}, {name}: {result:?}"),
            }
        }
    }
}

const DISABLED_BUTTON: NodeId = NodeId(2); // page, button
#[test]
fn a_disabled_button_takes_no_press() {
    let mut view = View::new(&LIMITS);
    let mut builder = view.builder();
    builder.open(Role::Page);
    builder.open(Role::Button);
    builder.bind(BINDING);
    builder.state(State::Disabled);
    builder.close();
    builder.close();
    finish(&mut view, builder);
    assert_eq!(view.act(DISABLED_BUTTON, Act::Press), None);
}

const DISABLED_OPTION: NodeId = NodeId(5); // page, disabled select, sibling select, region, option
#[test]
fn an_option_of_a_disabled_select_takes_no_press() {
    let mut view = View::new(&LIMITS);
    let mut builder = view.builder();
    builder.open(Role::Page);
    builder.open(Role::Select);
    builder.state(State::Disabled);
    builder.open(Role::Select);
    builder.close();
    builder.open(Role::Region);
    builder.open(Role::Option);
    builder.bind(BINDING);
    builder.close();
    builder.close();
    builder.close();
    builder.close();
    finish(&mut view, builder);
    assert_eq!(view.act(DISABLED_OPTION, Act::Press), None);
}

const INNER_OPTION: NodeId = NodeId(4); // page, disabled select, enabled select, option
#[test]
fn an_option_uses_its_nearest_selects_disabled_state() {
    let mut view = View::new(&LIMITS);
    let mut builder = view.builder();
    builder.open(Role::Page);
    builder.open(Role::Select);
    builder.state(State::Disabled);
    builder.open(Role::Select);
    builder.open(Role::Option);
    builder.bind(BINDING);
    builder.close();
    builder.close();
    builder.close();
    builder.close();
    finish(&mut view, builder);
    assert_eq!(view.act(INNER_OPTION, Act::Press), Some(BoundAct { binding: BINDING, act: Act::Press }));
}

const BOUND_DRAWING: NodeId = NodeId(1); // drawing
#[test]
fn drawing_data_past_its_bound_is_dropped() {
    let mut view = View::new(&LIMITS);
    let mut builder = view.builder();
    builder.drawing(DrawingKind { owner: BINDING.owner, kind: 1 }, b"Activity", b"", 8);
    builder.bind(BINDING);
    builder.close();
    finish(&mut view, builder);
    assert_eq!(view.act(BOUND_DRAWING, Act::Drawing { data: Box::from(*b"123456789") }), None);
}
