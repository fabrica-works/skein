use super::{DRAWING, button, drawing, end, main, page};
use crate::{
    mapping::Primary,
    page::{Event, Op, Page},
    tests::fixtures::{self, LIMITS},
};
use skein_ui::{Act, Builder, DrawingKind, Modifier, NodeId, Patch, Role, Run, Shortcut, State, View};

// Preorder: page, main, button, link, field, area, check, choice, dialog,
// disabled button, primary button, alt button, plain button, duplicate primary,
// unbound button, external link, menu item, menu.
const EVENT_BUTTON: NodeId = NodeId(3);
const EVENT_LINK: NodeId = NodeId(4);
const EVENT_FIELD: NodeId = NodeId(5);
const EVENT_AREA: NodeId = NodeId(6);
const EVENT_CHECK: NodeId = NodeId(7);
const EVENT_CHOICE: NodeId = NodeId(8);
const EVENT_DIALOG: NodeId = NodeId(9);
const EVENT_DISABLED: NodeId = NodeId(10);
const EVENT_PRIMARY: NodeId = NodeId(11);
const EVENT_ALT: NodeId = NodeId(12);
const EVENT_PLAIN: NodeId = NodeId(13);
const EVENT_UNBOUND: NodeId = NodeId(15);
const EVENT_EXTERNAL: NodeId = NodeId(16);
const EVENT_ITEM: NodeId = NodeId(17);
const EVENT_MENU: NodeId = NodeId(18);

fn events(builder: &mut Builder) {
    main(builder);
    button(builder, b"Press");
    builder.open(Role::Link);
    builder.link(fixtures::binding(2));
    builder.close();
    for role in [Role::TextField, Role::TextArea] {
        builder.open(role);
        builder.bind(fixtures::binding(3));
        builder.state(State::SubmitsOnEnter);
        builder.close();
    }
    builder.open(Role::Check);
    builder.bind(fixtures::binding(4));
    builder.state(State::Pressed);
    builder.close();
    builder.open(Role::Choice);
    builder.bind(fixtures::binding(5));
    builder.state(State::Selected);
    builder.close();
    builder.open(Role::Dialog);
    builder.close();
    builder.open(Role::Button);
    builder.bind(fixtures::binding(6));
    builder.state(State::Disabled);
    builder.close();
    for (key, modifier) in
        [(b'k', Modifier::Primary), (b'k', Modifier::Alt), (b'p', Modifier::Plain), (b'k', Modifier::Primary)]
    {
        builder.open(Role::Button);
        builder.bind(fixtures::binding(7));
        builder.shortcut(Shortcut { key, modifier });
        builder.close();
    }
    builder.open(Role::Button);
    builder.close();
    builder.open(Role::Link);
    builder.external(b"https://example.com/");
    builder.close();
    builder.open(Role::MenuItem);
    builder.bind(fixtures::binding(8));
    builder.close();
    builder.open(Role::Menu);
    builder.close();
    end(builder);
}

#[derive(Clone, Copy, Debug)]
enum KeyCase {
    Primary,
    Other,
    Shift,
    ControlAlt,
    AltComposed,
    Plain,
}
#[derive(Clone, Copy, Debug)]
enum Case {
    Click(NodeId, bool),
    Input(bool),
    CompositionEnd,
    Enter(NodeId, bool, bool),
    Change(NodeId),
    Cancel,
    Key(KeyCase),
}
#[derive(Clone, Copy)]
enum Expected {
    None,
    Press(NodeId),
    Input,
    Submit(NodeId),
    Toggle(NodeId),
}

const EVENTS: &[(Primary, Case, Expected)] = &[
    (Primary::Control, Case::Click(EVENT_BUTTON, false), Expected::Press(EVENT_BUTTON)),
    (Primary::Control, Case::Click(EVENT_LINK, false), Expected::Press(EVENT_LINK)),
    (Primary::Control, Case::Click(EVENT_LINK, true), Expected::None),
    (Primary::Control, Case::Click(EVENT_DISABLED, false), Expected::None),
    (Primary::Control, Case::Click(EVENT_UNBOUND, false), Expected::None),
    (Primary::Control, Case::Click(EVENT_EXTERNAL, false), Expected::None),
    (Primary::Control, Case::Click(EVENT_ITEM, false), Expected::Press(EVENT_ITEM)),
    (Primary::Control, Case::Click(EVENT_MENU, false), Expected::None),
    (Primary::Control, Case::Input(false), Expected::Input),
    (Primary::Control, Case::Input(true), Expected::None),
    (Primary::Control, Case::Enter(EVENT_FIELD, false, false), Expected::Submit(EVENT_FIELD)),
    (Primary::Control, Case::Enter(EVENT_AREA, false, false), Expected::Submit(EVENT_AREA)),
    (Primary::Control, Case::Enter(EVENT_AREA, true, false), Expected::None),
    (Primary::Control, Case::Enter(EVENT_FIELD, false, true), Expected::None),
    (Primary::Control, Case::Change(EVENT_CHECK), Expected::Toggle(EVENT_CHECK)),
    (Primary::Control, Case::Change(EVENT_CHOICE), Expected::Toggle(EVENT_CHOICE)),
    (Primary::Control, Case::Cancel, Expected::None),
    (Primary::Control, Case::Key(KeyCase::Primary), Expected::Press(EVENT_PRIMARY)),
    (Primary::Meta, Case::Key(KeyCase::Primary), Expected::Press(EVENT_PRIMARY)),
    (Primary::Control, Case::Key(KeyCase::Other), Expected::None),
    (Primary::Meta, Case::Key(KeyCase::Other), Expected::None),
    (Primary::Control, Case::Key(KeyCase::Shift), Expected::None),
    (Primary::Meta, Case::Key(KeyCase::Shift), Expected::None),
    (Primary::Control, Case::Key(KeyCase::ControlAlt), Expected::None),
    (Primary::Meta, Case::Key(KeyCase::ControlAlt), Expected::None),
    (Primary::Control, Case::Key(KeyCase::AltComposed), Expected::Press(EVENT_ALT)),
    (Primary::Meta, Case::Key(KeyCase::AltComposed), Expected::Press(EVENT_ALT)),
    (Primary::Control, Case::Key(KeyCase::Plain), Expected::Press(EVENT_PLAIN)),
    (Primary::Control, Case::CompositionEnd, Expected::Input),
];

fn key_event(primary: Primary, case: KeyCase, in_field: bool) -> Event {
    let mut event = Event::Key {
        key: Box::from(b"k".as_slice()),
        code: Box::from(b"KeyK".as_slice()),
        control: false,
        meta: false,
        alt: false,
        shift: false,
        in_field,
    };
    if let Event::Key { key, code, control, meta, alt, shift, .. } = &mut event {
        match case {
            KeyCase::Primary | KeyCase::Shift => {
                *control = primary == Primary::Control;
                *meta = primary == Primary::Meta;
                *shift = matches!(case, KeyCase::Shift);
            }
            KeyCase::Other => {
                *control = primary == Primary::Meta;
                *meta = primary == Primary::Control;
            }
            KeyCase::ControlAlt => {
                *control = true;
                *alt = true;
            }
            KeyCase::AltComposed => {
                *key = Box::from("˚".as_bytes());
                *alt = true;
            }
            KeyCase::Plain => {
                *key = Box::from(b"p".as_slice());
                *code = Box::from(b"KeyP".as_slice());
            }
        }
    }
    event
}
fn event(case: Case, primary: Primary) -> Event {
    match case {
        Case::Click(node, modifier) => Event::Click { node, modifier },
        Case::Input(composing) => Event::Input { node: EVENT_FIELD, value: Box::from(b"Typed".as_slice()), composing },
        Case::CompositionEnd => Event::CompositionEnd { node: EVENT_FIELD, value: Box::from(b"Typed".as_slice()) },
        Case::Enter(node, shift, composing) => Event::Enter { node, shift, composing },
        Case::Change(node) => Event::Change { node },
        Case::Cancel => Event::Cancel { dialog: EVENT_DIALOG },
        Case::Key(case) => key_event(primary, case, false),
    }
}
fn expected(expected: Expected) -> Option<(NodeId, Act)> {
    match expected {
        Expected::None => None,
        Expected::Press(node) => Some((node, Act::Press)),
        Expected::Input => Some((EVENT_FIELD, Act::Input { value: Box::from(b"Typed".as_slice()) })),
        Expected::Submit(node) => Some((node, Act::Submit)),
        Expected::Toggle(node) => Some((node, Act::Toggle)),
    }
}

#[test]
fn each_event_gives_its_act() {
    let mut view = View::new(&LIMITS);
    let patches = fixtures::patches(&mut view, 0, events);
    for (primary, case, want) in EVENTS {
        let mut page = Page::new(fixtures::look(), &LIMITS, b"/", *primary);
        page.apply(&patches, &mut Vec::new());
        assert_eq!(page.act(event(*case, *primary), &mut Vec::new()), expected(*want), "{primary:?}: {case:?}");
    }
}

#[test]
fn a_dialogs_cancel_keeps_it_open() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, events), &mut Vec::new());
    page.act(Event::Cancel { dialog: EVENT_DIALOG }, &mut ops);
    assert_eq!(
        ops,
        vec![Op::Attributes {
            node: EVENT_DIALOG,
            element: crate::dom::Element {
                tag: "dialog".into(),
                attributes: vec![("data-node".into(), "9".into())],
                children: vec![],
            }
        }]
    );
}

#[test]
fn a_shortcut_with_no_modifier_gives_none_while_a_field_has_focus() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, events), &mut Vec::new());
    assert_eq!(page.act(key_event(Primary::Control, KeyCase::Plain, true), &mut Vec::new()), None);
}

#[test]
fn a_composition_end_is_reported_once() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, events), &mut Vec::new());
    page.act(event(Case::CompositionEnd, Primary::Control), &mut Vec::new());
    assert_eq!(page.act(event(Case::Input(false), Primary::Control), &mut Vec::new()), None);
}

#[test]
fn a_check_is_put_back_to_the_tree_after_its_change() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, events), &mut Vec::new());
    page.act(Event::Change { node: EVENT_CHECK }, &mut ops);
    assert_eq!(ops, vec![Op::Choice { node: EVENT_CHECK, checked: true }]);
}

// Preorder: page, main, menu, first item, disabled item, last item.
const MENU: NodeId = NodeId(3);
const FIRST_MENU_ITEM: NodeId = NodeId(4);
const LAST_MENU_ITEM: NodeId = NodeId(6);
fn menu(builder: &mut Builder) {
    main(builder);
    builder.open(Role::Menu);
    builder.name(b"Actions");
    for disabled in [false, true, false] {
        builder.open(Role::MenuItem);
        builder.name(b"Item");
        builder.bind(fixtures::binding(1));
        if disabled {
            builder.state(State::Disabled);
        }
        builder.close();
    }
    builder.close();
    end(builder);
}
fn plain_key(key: &[u8]) -> Event {
    Event::Key {
        key: key.into(),
        code: Box::new([]),
        control: false,
        meta: false,
        alt: false,
        shift: false,
        in_field: false,
    }
}

#[test]
fn a_menu_opens_without_a_step() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, menu), &mut Vec::new());
    page.act(Event::Click { node: MENU, modifier: false }, &mut Vec::new());
    assert_eq!(page.html(), include_str!("../golden/menu-open.html").trim_end());
}

#[test]
fn escape_closes_an_open_menu() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, menu), &mut Vec::new());
    page.act(Event::Click { node: MENU, modifier: false }, &mut Vec::new());
    page.act(plain_key(b"Escape"), &mut Vec::new());
    assert_eq!(page.html(), include_str!("../golden/menu-closed.html").trim_end());
}

#[test]
fn menu_arrows_skip_disabled_items() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, menu), &mut Vec::new());
    page.act(Event::Click { node: MENU, modifier: false }, &mut Vec::new());
    page.focused(FIRST_MENU_ITEM);
    page.act(plain_key(b"ArrowDown"), &mut ops);
    assert_eq!(ops, vec![Op::Focus { node: LAST_MENU_ITEM }]);
}

#[test]
fn pressing_a_menu_item_closes_the_menu() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, menu), &mut Vec::new());
    page.act(Event::Click { node: MENU, modifier: false }, &mut Vec::new());
    page.act(Event::Click { node: FIRST_MENU_ITEM, modifier: false }, &mut Vec::new());
    assert_eq!(page.html(), include_str!("../golden/menu-closed.html").trim_end());
}

const LOG: NodeId = NodeId(3); // page, main, log, text

#[test]
fn scrolling_a_log_away_stops_its_following() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            b.open(Role::Log);
            b.text(Run::Plain, b"Old");
            b.close();
            end(b);
        }),
        &mut Vec::new(),
    );
    page.act(Event::Scrolled { node: LOG, top: 0, height: 100, viewport: 20 }, &mut Vec::new());
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            main(b);
            b.open(Role::Log);
            b.text(Run::Plain, b"New");
            b.close();
            end(b);
        }),
        &mut ops,
    );
    let following: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            Op::ScrollEnd { node } => Some(*node),
            _ => None,
        })
        .collect();
    assert_eq!(following, vec![]);
}

#[test]
fn a_choice_is_put_back_to_the_tree_after_its_change() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, events), &mut Vec::new());
    page.act(Event::Change { node: EVENT_CHOICE }, &mut ops);
    assert_eq!(ops, vec![Op::Choice { node: EVENT_CHOICE, checked: true }]);
}

#[test]
fn a_dialog_closed_by_the_browser_is_shown_again() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, events), &mut Vec::new());
    page.act(Event::Closed { dialog: EVENT_DIALOG }, &mut ops);
    assert_eq!(
        ops,
        vec![Op::Attributes {
            node: EVENT_DIALOG,
            element: crate::dom::Element {
                tag: "dialog".into(),
                attributes: vec![("data-node".into(), "9".into())],
                children: vec![],
            }
        }]
    );
}

#[test]
fn a_log_follows_its_end() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            b.open(Role::Log);
            b.close();
            end(b);
        }),
        &mut ops,
    );
    let following: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            Op::ScrollEnd { node } => Some(*node),
            _ => None,
        })
        .collect();
    assert_eq!(following, vec![LOG]);
}

fn act_scene(_: DrawingKind, data: &[u8], width: u16, height: u16, scene: &mut skein_ui::scene::Scene) -> bool {
    use skein_ui::scene::Point;
    scene.rect(Point { x: 0, y: 0 }, Point { x: width, y: height }, 0, &[]);
    if data == b"undrawn" {
        scene.scene_act(b"bg");
        return false;
    }
    if data == b"background" { scene.scene_act(b"bg") } else { scene.act(b"bar") }
}
fn drawing_page(data: &[u8]) -> Page<'static> {
    let look = crate::Look { draw: act_scene, ..fixtures::look() };
    let (mut page, mut view) = (Page::new(look, &LIMITS, b"/", Primary::Control), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, |b| drawing(b, data)), &mut Vec::new());
    let mut attribute = skein_ui::Made::of(view.tree().get(2).expect("fixture drawing")).attribute;
    attribute.bound = true;
    page.apply(&[Patch::Attribute { node: DRAWING, attribute }], &mut Vec::new());
    page.resized(DRAWING, 100, 50, &mut Vec::new());
    page
}

#[test]
fn a_mark_press_reports_its_bytes() {
    let mut page = drawing_page(b"mark");
    assert_eq!(
        page.act(Event::Drawing { node: DRAWING, under: Box::new([0]), x: 7, y: 9 }, &mut Vec::new()),
        Some((DRAWING, Act::Drawing { data: Box::from(b"bar".as_slice()) }))
    );
}

#[test]
fn a_background_press_appends_its_point() {
    let mut page = drawing_page(b"background");
    assert_eq!(
        page.act(Event::Drawing { node: DRAWING, under: Box::new([]), x: 7, y: 9 }, &mut Vec::new()),
        Some((DRAWING, Act::Drawing { data: Box::new([b'b', b'g', 7, 0, 9, 0]) }))
    );
}

#[test]
fn an_undrawn_scene_does_not_act() {
    let mut page = drawing_page(b"undrawn");
    assert_eq!(page.act(Event::Drawing { node: DRAWING, under: Box::new([]), x: 7, y: 9 }, &mut Vec::new()), None);
}

#[test]
fn an_enter_keeps_its_input_after_a_composition() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, events), &mut Vec::new());
    page.act(event(Case::CompositionEnd, Primary::Control), &mut Vec::new());
    page.act(Event::Enter { node: EVENT_FIELD, shift: false, composing: false }, &mut Vec::new());
    assert_eq!(
        page.act(event(Case::Input(false), Primary::Control), &mut Vec::new()),
        Some((EVENT_FIELD, Act::Input { value: Box::from(b"Typed".as_slice()) }))
    );
}

const ESCAPE_DIALOG: NodeId = NodeId(3); // page, main, dialog
const ESCAPE_BUTTON: NodeId = NodeId(5); // page, main, dialog, disabled escape button, escape button
fn dialog_escape(b: &mut Builder) {
    main(b);
    b.open(Role::Dialog);
    for disabled in [true, false] {
        b.open(Role::Button);
        b.bind(fixtures::binding(1));
        b.shortcut(Shortcut::ESCAPE);
        if disabled {
            b.state(State::Disabled);
        }
        b.close();
    }
    b.close();
    end(b);
}

#[test]
fn escape_presses_the_dialogs_escape_button() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, dialog_escape), &mut Vec::new());
    assert_eq!(page.act(Event::Cancel { dialog: ESCAPE_DIALOG }, &mut Vec::new()), Some((ESCAPE_BUTTON, Act::Press)));
}
