use skein_ui::{Builder, DrawingKind, Key, NodeId, Patch, Role, Run, State, View};

use super::fixtures::{self, LIMITS, OWNER};
use crate::{
    mapping::Primary,
    page::{Op, Page},
};

fn page() -> Page<'static> {
    Page::new(fixtures::look(), &LIMITS, b"/bundle/digest/", Primary::Control)
}
const MAIN: NodeId = NodeId(2); // page, main
fn main(builder: &mut Builder) {
    builder.open(Role::Page);
    builder.open(Role::Main);
}
fn end(builder: &mut Builder) {
    builder.close();
    builder.close();
}
fn field(builder: &mut Builder, value: &[u8], written: u64) {
    builder.open(Role::TextField);
    builder.name(b"Title");
    builder.value(value, written);
    builder.close();
}
const BUTTON: NodeId = NodeId(3); // page, main, button
fn button(builder: &mut Builder, name: &[u8]) {
    builder.open(Role::Button);
    builder.name(name);
    builder.bind(fixtures::binding(1));
    builder.close();
}
fn list(builder: &mut Builder, keys: &[u64]) {
    main(builder);
    builder.open(Role::List);
    for key in keys {
        builder.open(Role::Item);
        builder.key(Key { owner: OWNER, kind: 0, a: *key, b: 0 });
        builder.text(Run::Plain, if *key == 1 { b"One" } else { b"Two" });
        builder.close();
    }
    builder.close();
    end(builder);
}
const DRAWING: NodeId = NodeId(3); // page, main, drawing
fn drawing(builder: &mut Builder, data: &[u8]) {
    main(builder);
    builder.drawing(DrawingKind { owner: OWNER, kind: 0 }, b"Chart", data, 128);
    builder.text(Run::Plain, b"Fallback");
    builder.close();
    end(builder);
}

#[test]
fn an_insert_builds_the_golden_page() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            button(b, b"Go");
            field(b, b"Old", 0);
            end(b);
        }),
        &mut Vec::new(),
    );
    assert_eq!(page.html(), include_str!("golden/insert.html").trim_end());
}

#[test]
fn a_remove_takes_its_subtree() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            b.open(Role::Region);
            button(b, b"Go");
            b.close();
            end(b);
        }),
        &mut Vec::new(),
    );
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            main(b);
            end(b);
        }),
        &mut Vec::new(),
    );
    assert_eq!(page.html(), include_str!("golden/remove.html").trim_end());
}

#[test]
fn a_move_keeps_the_element() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, |b| list(b, &[1, 2])), &mut Vec::new());
    page.apply(&fixtures::patches(&mut view, 1, |b| list(b, &[2, 1])), &mut Vec::new());
    assert_eq!(page.html(), include_str!("golden/move.html").trim_end());
}

#[test]
fn a_value_patch_writes_the_field() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            field(b, b"Old", 0);
            end(b);
        }),
        &mut Vec::new(),
    );
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            main(b);
            field(b, b"New", 1);
            end(b);
        }),
        &mut Vec::new(),
    );
    assert_eq!(page.html(), include_str!("golden/value.html").trim_end());
}

#[test]
fn a_text_patch_replaces_the_runs() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            b.text(Run::Plain, b"Old");
            end(b);
        }),
        &mut Vec::new(),
    );
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            main(b);
            b.text(Run::Strong, b"New");
            end(b);
        }),
        &mut Vec::new(),
    );
    assert_eq!(page.html(), include_str!("golden/text.html").trim_end());
}

#[test]
fn an_attribute_patch_updates_the_element() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            button(b, b"Old");
            end(b);
        }),
        &mut Vec::new(),
    );
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            main(b);
            b.open(Role::Button);
            b.name(b"New");
            b.state(State::Disabled);
            b.close();
            end(b);
        }),
        &mut Vec::new(),
    );
    assert_eq!(page.html(), include_str!("golden/attribute.html").trim_end());
}

#[test]
fn an_attribute_patch_keeps_a_tag_it_can_update() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            button(b, b"Old");
            end(b);
        }),
        &mut Vec::new(),
    );
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            main(b);
            button(b, b"New");
            end(b);
        }),
        &mut ops,
    );
    let kinds: Vec<_> = ops
        .iter()
        .map(|op| match op {
            Op::Attributes { .. } => "attributes",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["attributes"]);
}

#[test]
fn a_changed_tag_replaces_the_element() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            end(b);
        }),
        &mut Vec::new(),
    );
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            b.open(Role::Region);
            b.open(Role::Main);
            b.close();
            b.close();
        }),
        &mut ops,
    );
    let kinds: Vec<_> = ops
        .iter()
        .map(|op| match op {
            Op::Replace { .. } => "replace",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["replace"]);
}

// Preorder: page, main, dialog, region, button.
const FOCUS_DIALOG: NodeId = NodeId(3);
const FOCUS_SUBTREE: NodeId = NodeId(4);
const FOCUS_BUTTON: NodeId = NodeId(5);

#[test]
fn a_removal_taking_the_focus_moves_it_to_an_ancestor() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            b.open(Role::Dialog);
            b.open(Role::Region);
            button(b, b"Go");
            b.close();
            b.close();
            end(b);
        }),
        &mut Vec::new(),
    );
    page.focused(FOCUS_BUTTON);
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            main(b);
            b.open(Role::Dialog);
            b.close();
            end(b);
        }),
        &mut ops,
    );
    assert_eq!(ops, vec![Op::Remove { node: FOCUS_SUBTREE }, Op::Focus { node: FOCUS_DIALOG }]);
}

#[test]
fn a_removal_taking_the_focus_falls_back_to_the_main() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            button(b, b"Go");
            end(b);
        }),
        &mut Vec::new(),
    );
    page.focused(BUTTON);
    page.apply(
        &fixtures::patches(&mut view, 1, |b| {
            main(b);
            end(b);
        }),
        &mut ops,
    );
    assert_eq!(ops, vec![Op::Remove { node: BUTTON }, Op::Focus { node: MAIN }]);
}

const MISSING_NODE: NodeId = NodeId(99); // never allocated in the fixture

#[test]
#[should_panic(expected = "patch names a held node")]
fn a_patch_naming_no_held_node_panics() {
    page().apply(&[Patch::Remove { node: MISSING_NODE }], &mut Vec::new());
}

#[test]
fn a_drawing_shows_its_fallback_before_its_first_resize() {
    let (mut page, mut view) =
        (Page::new(fixtures::drawing_look(), &LIMITS, b"/", Primary::Control), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, |b| drawing(b, b"one")), &mut Vec::new());
    assert_eq!(page.html(), include_str!("golden/fallback.html").trim_end());
}

#[test]
fn a_resize_draws_the_scene_and_keeps_the_fallback() {
    let (mut page, mut view) =
        (Page::new(fixtures::drawing_look(), &LIMITS, b"/", Primary::Control), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, |b| drawing(b, b"one")), &mut Vec::new());
    page.resized(DRAWING, 100, 50, &mut Vec::new());
    assert_eq!(page.html(), include_str!("golden/drawing.html").trim_end());
}

#[test]
fn changed_drawing_data_draws_again_at_its_last_size() {
    let (mut page, mut view) =
        (Page::new(fixtures::drawing_look(), &LIMITS, b"/", Primary::Control), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, |b| drawing(b, b"one")), &mut Vec::new());
    page.resized(DRAWING, 100, 50, &mut Vec::new());
    page.apply(&fixtures::patches(&mut view, 1, |b| drawing(b, b"two")), &mut Vec::new());
    assert_eq!(page.html(), include_str!("golden/drawing-changed.html").trim_end());
}

#[test]
fn a_refused_drawing_keeps_its_fallback_visible() {
    let (mut page, mut view) =
        (Page::new(fixtures::drawing_look(), &LIMITS, b"/", Primary::Control), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, |b| drawing(b, b"refuse")), &mut Vec::new());
    page.resized(DRAWING, 100, 50, &mut Vec::new());
    assert_eq!(page.html(), include_str!("golden/fallback.html").trim_end());
}

// Preorder: page, main, first link, second link.
const FIRST_LINK: NodeId = NodeId(3);
const SECOND_LINK: NodeId = NodeId(4);

#[test]
fn links_are_reported_in_preorder() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            for n in 1..=2 {
                b.open(Role::Link);
                b.link(fixtures::binding(n));
                b.close();
            }
            end(b);
        }),
        &mut Vec::new(),
    );
    assert_eq!(page.links(), &[FIRST_LINK, SECOND_LINK]);
}

#[test]
fn a_links_href_is_the_owners_path() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            b.open(Role::Link);
            b.link(fixtures::binding(1));
            b.close();
            end(b);
        }),
        &mut Vec::new(),
    );
    page.set_href(FIRST_LINK, b"/notes/1", &mut Vec::new());
    assert_eq!(page.html(), include_str!("golden/href.html").trim_end());
}

mod events;

const FOCUS_HEADING: NodeId = NodeId(3); // page, main, heading
fn focus_heading(b: &mut Builder, count: u64) {
    main(b);
    b.open(Role::Heading { level: 1 });
    b.focus(count);
    b.close();
    end(b);
}

#[test]
fn a_focus_request_focuses_its_node() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, |b| focus_heading(b, 1)), &mut ops);
    assert_eq!(ops.last(), Some(&Op::Focus { node: FOCUS_HEADING }));
}

#[test]
fn a_moved_focus_count_focuses_again() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, |b| focus_heading(b, 1)), &mut Vec::new());
    page.apply(&fixtures::patches(&mut view, 1, |b| focus_heading(b, 2)), &mut ops);
    assert_eq!(ops.last(), Some(&Op::Focus { node: FOCUS_HEADING }));
}

#[test]
fn an_unchanged_focus_count_does_not_focus_again() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, |b| focus_heading(b, 1)), &mut Vec::new());
    page.apply(&fixtures::patches(&mut view, 1, |b| focus_heading(b, 1)), &mut ops);
    assert_eq!(ops, []);
}

#[test]
fn a_described_cell_keeps_its_description_after_its_children() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            b.open(Role::Cell);
            b.description(b"Help");
            b.text(Run::Plain, b"Value");
            b.close();
            end(b);
        }),
        &mut Vec::new(),
    );
    assert_eq!(
        page.html(),
        r#"<div><main data-node="2"><td data-node="3" aria-describedby="skein-ui-description-3"><span data-node="4">Value</span><span id="skein-ui-description-3">Help</span></td></main></div>"#
    );
}

#[test]
fn a_changed_description_updates_its_companion() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    for (count, words) in [(0, b"Old".as_slice()), (1, b"Help & <b>")] {
        page.apply(
            &fixtures::patches(&mut view, count, |b| {
                main(b);
                b.open(Role::TextField);
                b.description(words);
                b.close();
                end(b);
            }),
            &mut Vec::new(),
        );
    }
    assert_eq!(
        page.html(),
        r#"<div><main data-node="2"><input data-node="3" type="text" aria-describedby="skein-ui-description-3"><span id="skein-ui-description-3">Help &amp; &lt;b&gt;</span></main></div>"#
    );
}

const COUNTED_FIRST: NodeId = NodeId(4); // page, main, list, first item
const COUNTED_SECOND: NodeId = NodeId(5); // page, main, list, first item, second item
fn counted_items(b: &mut Builder, count: Option<u32>) {
    main(b);
    b.open(Role::List);
    if let Some(count) = count {
        b.count(count);
    }
    for position in [1, 2] {
        b.open(Role::Item);
        b.position(position);
        b.close();
    }
    b.close();
    end(b);
}

#[test]
fn a_changed_count_sets_each_items_setsize_again() {
    let (mut page, mut view, mut ops) = (page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, |b| counted_items(b, Some(10))), &mut Vec::new());
    page.apply(&fixtures::patches(&mut view, 1, |b| counted_items(b, Some(20))), &mut ops);
    let sizes: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Attributes { node, element } => element.attribute("aria-setsize").map(|size| (*node, size)),
            Op::Insert { .. }
            | Op::Remove { .. }
            | Op::Move { .. }
            | Op::Replace { .. }
            | Op::Text { .. }
            | Op::Value { .. }
            | Op::Choice { .. }
            | Op::Href { .. }
            | Op::Focus { .. }
            | Op::ScrollEnd { .. }
            | Op::Component { .. } => None,
        })
        .collect();
    assert_eq!(sizes, [(COUNTED_FIRST, "20"), (COUNTED_SECOND, "20")]);
}

#[test]
fn a_removed_count_removes_each_items_setsize() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, |b| counted_items(b, Some(10))), &mut Vec::new());
    page.apply(&fixtures::patches(&mut view, 1, |b| counted_items(b, None)), &mut Vec::new());
    assert_eq!(
        page.html(),
        r#"<div><main data-node="2"><ul data-node="3"><li data-node="4" aria-posinset="1"></li><li data-node="5" aria-posinset="2"></li></ul></main></div>"#
    );
}

#[test]
fn an_options_description_follows_its_select() {
    let (mut page, mut view) = (page(), View::new(&LIMITS));
    page.apply(
        &fixtures::patches(&mut view, 0, |b| {
            main(b);
            b.open(Role::Select);
            b.open(Role::Option);
            b.name(b"One");
            b.description(b"Help");
            b.close();
            b.close();
            end(b);
        }),
        &mut Vec::new(),
    );
    assert_eq!(
        page.html(),
        r#"<div><main data-node="2"><select data-node="3"><option data-node="4" aria-describedby="skein-ui-description-4">One</option></select><span id="skein-ui-description-4">Help</span></main></div>"#
    );
}

const COMPONENT_BODY: NodeId = NodeId(3); // page, main, body
fn component_body(builder: &mut Builder, value: &[u8], written: u64) {
    main(builder);
    builder.open(Role::TextArea);
    builder.name(b"Body");
    builder.style(skein_ui::Style { owner: OWNER, token: 0 });
    builder.value(value, written);
    builder.close();
    end(builder);
}
fn component_page() -> Page<'static> {
    Page::new(
        crate::Look {
            components: &[(OWNER, Role::TextArea, skein_ui::Style { owner: OWNER, token: 0 })],
            ..fixtures::look()
        },
        &LIMITS,
        b"/",
        Primary::Control,
    )
}
#[test]
fn a_component_node_keeps_its_host_and_data_node() {
    let (mut page, mut view) = (component_page(), View::new(&LIMITS));
    page.apply(&fixtures::patches(&mut view, 0, |b| component_body(b, b"Old", 0)), &mut Vec::new());
    page.apply(&fixtures::patches(&mut view, 1, |b| component_body(b, b"New", 1)), &mut Vec::new());
    assert_eq!(
        page.html(),
        r#"<div><main data-node="2"><textarea data-node="3" aria-label="Body" class="test-mark" data-component="0">New</textarea></main></div>"#
    );
}
#[test]
fn a_component_patch_receives_the_new_value() {
    let (mut page, mut view, mut ops) = (component_page(), View::new(&LIMITS), Vec::new());
    page.apply(&fixtures::patches(&mut view, 0, |b| component_body(b, b"Old", 0)), &mut Vec::new());
    page.apply(&fixtures::patches(&mut view, 1, |b| component_body(b, b"New", 1)), &mut ops);
    let values: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Component { node, made, .. } => Some((*node, made.value.as_ref().map(|value| value.text.as_ref()))),
            _ => None,
        })
        .collect();
    assert_eq!(values, vec![(COMPONENT_BODY, Some(b"New".as_slice()))]);
}
