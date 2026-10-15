#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "the independent test model uses ordinary Rust containers and iterator queries"
)]

use crate::Binding;
use crate::Limits;
use crate::{Builder, DrawingKind, Key, Made, NodeId, Owner, Patch, Role, Run, State, Tree, View};

use skein_lib::{Queue, Rng};

const LIMITS: Limits = Limits {
    nodes: 64,
    depth: 10,
    text_per_node: 32,
    text_bytes: 1024,
    drawing_bytes: 8,
    styles_per_node: 2,
    patches: 384,
    bindings: 32,
    slots: 8,
    markdown_depth: 2,
    scene_marks: 16,
    scene_bytes: 256,
};

#[derive(Debug)]
struct Entry {
    node: Made,
    parent: NodeId,
}

fn entries(nodes: &[Made], parent: NodeId) -> Vec<Entry> {
    let mut rows = Vec::new();
    let mut stack: Vec<(NodeId, u32)> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let index = u32::try_from(index).expect("small test tree");
        while stack.last().is_some_and(|(_, end)| index >= *end) {
            stack.pop();
        }
        let parent = stack.last().map_or(parent, |(id, _)| *id);
        rows.push(Entry { node: node.clone(), parent });
        stack.push((node.id, index.checked_add(node.size).expect("small subtree")));
    }
    rows
}

fn position(rows: &[Entry], id: NodeId) -> usize {
    rows.iter().position(|row| row.node.id == id).expect("patch names a present node")
}

fn sizes(rows: &mut [Entry]) {
    for index in (0..rows.len()).rev() {
        let id = rows[index].node.id;
        let mut size = 1_u32;
        for child in rows.iter() {
            if child.parent == id {
                size = size.checked_add(child.node.size).expect("small tree");
            }
        }
        rows[index].node.size = size;
    }
}

fn insertion(rows: &[Entry], parent: NodeId, before: Option<NodeId>) -> usize {
    match before {
        Some(id) => position(rows, id),
        None => {
            if parent == NodeId(0) {
                rows.len()
            } else {
                let at = position(rows, parent);
                at.checked_add(usize::try_from(rows[at].node.size).expect("small subtree")).expect("small tree")
            }
        }
    }
}

fn apply(rows: &mut Vec<Entry>, patch: Patch) {
    match patch {
        Patch::Insert { parent, before, nodes } => {
            let at = insertion(rows, parent, before);
            drop(rows.splice(at..at, entries(&nodes, parent)));
        }
        Patch::Remove { node } => {
            let at = position(rows, node);
            let end = at.checked_add(usize::try_from(rows[at].node.size).expect("small subtree")).expect("small tree");
            drop(rows.drain(at..end));
        }
        Patch::Move { node, parent, before } => {
            let at = position(rows, node);
            let end = at.checked_add(usize::try_from(rows[at].node.size).expect("small subtree")).expect("small tree");
            let mut moved: Vec<_> = rows.drain(at..end).collect();
            moved[0].parent = parent;
            sizes(rows);
            let to = insertion(rows, parent, before);
            drop(rows.splice(to..to, moved));
        }
        Patch::Text { node, text } => {
            let at = position(rows, node);
            rows[at].node.text = text;
        }
        Patch::Attribute { node, attribute } => {
            let at = position(rows, node);
            rows[at].node.attribute = attribute;
        }
        Patch::Value { node, value } => {
            let at = position(rows, node);
            rows[at].node.value = value;
        }
    }
    sizes(rows);
}

fn equal(rows: &[Entry], tree: &Tree) {
    let model: Vec<_> = rows.iter().map(|row| row.node.clone()).collect();
    let wanted: Vec<_> = tree.nodes().iter().map(Made::of).collect();
    assert_eq!(model, wanted, "patches reproduce the platform-visible tree");
}

fn item(builder: &mut Builder, key: u64, changed: bool) {
    builder.open(Role::Item);
    builder.key(Key { owner: Owner { number: 65_536 }, kind: 1, a: key, b: 0 });
    builder.text(Run::Plain, if changed { b"new" } else { b"old" });
    builder.open(Role::Button);
    builder.name(b"Choose");
    builder.bind(Binding { owner: Owner { number: 65_536 }, token: skein_lib::Token::new(key) });
    builder.close();
    builder.close();
}

#[test]
fn every_patch_kind_preserves_ids_and_applies_as_a_tree_edit() {
    let mut view = View::new(&LIMITS);
    let mut out = Queue::with_capacity(LIMITS.patches);
    let mut model = Vec::new();
    let mut first = view.builder();
    first.open(Role::Page);
    first.open(Role::List);
    for key in [1, 2, 3] {
        item(&mut first, key, false);
    }
    first.close();
    first.open(Role::TextField);
    first.value(b"one", 1);
    first.close();
    first.close();
    view.finish(first, 1, &mut out);
    assert!(!view.stale(1));
    assert!(view.stale(2));
    let root = view.tree().get(0).expect("page").id;
    let retained = view.tree().get(2).expect("item one").id;
    let removed = view
        .tree()
        .nodes()
        .iter()
        .find(|node| node.binding.is_some_and(|binding| binding.token.raw() == 2))
        .expect("item two control")
        .id;
    let mut kinds = [false; 6];
    while let Some(patch) = out.pop() {
        kinds[0] = true;
        apply(&mut model, patch);
    }
    equal(&model, view.tree());
    let mut next = view.builder();
    next.open(Role::Region);
    next.state(State::Busy);
    next.open(Role::List);
    for key in [3, 4, 1] {
        item(&mut next, key, true);
    }
    next.close();
    next.open(Role::TextField);
    next.value(b"two", 2);
    next.close();
    next.close();
    view.finish(next, 2, &mut out);
    assert_eq!(view.tree().get(0).expect("root").id, root, "roots match even when their roles change");
    assert_eq!(view.tree().get(8).expect("item one").id, retained);
    assert!(view.bound(removed).is_none());
    assert!(out.len() <= LIMITS.patches);
    while let Some(patch) = out.pop() {
        match &patch {
            Patch::Insert { .. } => kinds[0] = true,
            Patch::Remove { .. } => kinds[1] = true,
            Patch::Move { .. } => kinds[2] = true,
            Patch::Text { .. } => kinds[3] = true,
            Patch::Attribute { .. } => kinds[4] = true,
            Patch::Value { .. } => kinds[5] = true,
        }
        apply(&mut model, patch);
    }
    assert_eq!(kinds, [true; 6]);
    equal(&model, view.tree());
    let again = view.builder();
    view.finish(again, 2, &mut out);
    assert!(out.is_empty(), "an unchanged shown count emits nothing");
    assert_eq!(view.tree().get(0).expect("last root").id, root);
}

#[test]
fn fields_keep_typing_and_binding_changes_are_private_while_drawings_change_whole() {
    let mut view = View::new(&LIMITS);
    let mut out = Queue::with_capacity(LIMITS.patches);
    for (shown, text, written, token) in
        [(1, b"draft".as_slice(), 1, 7), (2, b"typing".as_slice(), 1, 8), (3, b"saved".as_slice(), 2, 9)]
    {
        let mut builder = view.builder();
        builder.open(Role::TextArea);
        builder.value(text, written);
        builder.bind(Binding { owner: Owner { number: 65_536 }, token: skein_lib::Token::new(token) });
        builder.close();
        view.finish(builder, shown, &mut out);
        if shown == 2 {
            assert!(out.is_empty(), "neither private binding changes nor typing write the shell value");
        }
        if shown == 3 {
            assert_eq!(out.len(), 1);
            match out.pop().expect("owner value write") {
                Patch::Value { value, .. } => assert_eq!(value.expect("written value").text.as_ref(), b"saved"),
                Patch::Insert { .. }
                | Patch::Remove { .. }
                | Patch::Move { .. }
                | Patch::Text { .. }
                | Patch::Attribute { .. } => unreachable!("the field was already matched"),
            }
        }
        while out.pop().is_some() {}
    }
    let mut model = Vec::new();
    let mut view = View::new(&LIMITS);
    let kind = DrawingKind { owner: Owner { number: 1 }, kind: 1 };
    for data in [b"123".as_slice(), b"456"] {
        let mut builder = view.builder();
        builder.drawing(kind, b"Count", data, 3);
        builder.text(Run::Plain, b"a count");
        builder.close();
        view.finish(builder, u64::from(*data.first().expect("drawing has data")), &mut out);
        assert_eq!(out.len(), 1, "initial insertion or whole drawing data attribute");
        while let Some(patch) = out.pop() {
            apply(&mut model, patch);
        }
        equal(&model, view.tree());
    }
}

fn random_tree(builder: &mut Builder, rng: &mut Rng) {
    builder.open(if rng.chance(500) { Role::Page } else { Role::Region });
    let mut keys = [1_u64, 2, 3, 4, 5];
    for index in 0..keys.len() {
        let other = usize::try_from(rng.below(u64::try_from(keys.len()).expect("small array"))).expect("small index");
        keys.swap(index, other);
    }
    for key in keys {
        if rng.chance(500) {
            item(builder, key, rng.chance(500));
        }
        if rng.chance(500) {
            builder.open(if rng.chance(500) { Role::Paragraph } else { Role::Region });
            if rng.chance(500) {
                builder.state(State::Busy);
            }
            builder.text(Run::Plain, if rng.chance(500) { b"a" } else { b"b" });
            builder.close();
        }
    }
    builder.close();
}

#[test]
fn a_seeded_sweep_of_nested_trees_is_reproduced_by_the_independent_applier() {
    for seed in 0..24_u64 {
        let mut rng = Rng::new(seed);
        let mut view = View::new(&LIMITS);
        let mut model = Vec::new();
        let mut out = Queue::with_capacity(LIMITS.patches);
        for shown in 0..8_u64 {
            let old_ids: Vec<_> = model.iter().map(|entry: &Entry| entry.node.id).collect();
            let mut builder = view.builder();
            random_tree(&mut builder, &mut rng);
            view.finish(builder, shown, &mut out);
            assert!(out.len() <= LIMITS.patches);
            let mut trace = Vec::new();
            while let Some(patch) = out.pop() {
                apply(&mut model, patch.clone());
                trace.push(patch);
            }
            let ids: Vec<_> = model.iter().map(|entry| entry.node.id).collect();
            let expected: Vec<_> = view.tree().nodes().iter().map(|node| node.id).collect();
            assert_eq!(ids, expected, "seed {seed}, shown {shown}, old {old_ids:?}, patches {trace:?}");
            equal(&model, view.tree());
        }
    }
}

#[test]
fn a_changed_region_name_is_one_attribute_patch() {
    let mut view = View::new(&LIMITS);
    let mut out = Queue::with_capacity(LIMITS.patches);
    let mut builder = view.builder();
    builder.open(Role::Region);
    builder.region(crate::Region { owner: Owner { number: 65_536 }, number: 17 });
    builder.close();
    view.finish(builder, 1, &mut out);
    while out.pop().is_some() {}
    let id = view.tree().get(0).unwrap().id;
    let mut builder = view.builder();
    builder.open(Role::Region);
    builder.region(crate::Region { owner: Owner { number: 65_536 }, number: 18 });
    builder.close();
    view.finish(builder, 2, &mut out);
    assert_eq!(out.len(), 1);
    match out.pop().unwrap() {
        Patch::Attribute { node, attribute } => {
            assert_eq!(node, id);
            assert_eq!(attribute.region, Some(crate::Region { owner: Owner { number: 65_536 }, number: 18 }));
        }
        Patch::Insert { .. } | Patch::Remove { .. } | Patch::Move { .. } | Patch::Text { .. } | Patch::Value { .. } => {
            panic!("only the region attribute changed")
        }
    }
}

const FOCUSED_ROOT: NodeId = NodeId(1); // main

#[test]
fn a_moved_focus_request_is_an_attribute_patch() {
    let mut view = View::new(&LIMITS);
    let mut out = Queue::with_capacity(LIMITS.patches);
    for count in [1, 2] {
        let mut b = view.builder();
        b.open(Role::Main);
        b.focus(count);
        b.close();
        view.finish(b, count, &mut out);
        if count == 1 {
            while out.pop().is_some() {}
        }
    }
    assert_eq!(out.len(), 1);
    match out.pop().unwrap() {
        Patch::Attribute { node, attribute } => assert_eq!((node, attribute.focus), (FOCUSED_ROOT, Some(2))),
        patch @ (Patch::Insert { .. }
        | Patch::Remove { .. }
        | Patch::Move { .. }
        | Patch::Text { .. }
        | Patch::Value { .. }) => panic!("expected focus attribute, got {patch:?}"),
    }
}

const POSITIONED_ITEM: NodeId = NodeId(1); // item

#[test]
fn a_changed_position_is_one_attribute_patch() {
    let mut view = View::new(&LIMITS);
    let mut out = Queue::with_capacity(LIMITS.patches);
    for position in [1, 2] {
        let mut b = view.builder();
        b.open(Role::Item);
        b.position(position);
        b.close();
        view.finish(b, u64::from(position), &mut out);
        if position == 1 {
            while out.pop().is_some() {}
        }
    }
    assert_eq!(out.len(), 1);
    match out.pop().unwrap() {
        Patch::Attribute { node, attribute } => assert_eq!((node, attribute.position), (POSITIONED_ITEM, Some(2))),
        Patch::Insert { .. } | Patch::Remove { .. } | Patch::Move { .. } | Patch::Text { .. } | Patch::Value { .. } => {
            panic!("only position changed")
        }
    }
}

const SORTED_HEADER: NodeId = NodeId(1); // column header

#[test]
fn a_changed_sort_is_one_attribute_patch() {
    let mut view = View::new(&LIMITS);
    let mut out = Queue::with_capacity(LIMITS.patches);
    for (shown, sort) in [(1, crate::Sort::Ascending), (2, crate::Sort::Descending)] {
        let mut b = view.builder();
        b.open(Role::ColumnHeader);
        b.sort(sort);
        b.close();
        view.finish(b, shown, &mut out);
        if shown == 1 {
            while out.pop().is_some() {}
        }
    }
    assert_eq!(out.len(), 1);
    match out.pop().unwrap() {
        Patch::Attribute { node, attribute } => {
            assert_eq!((node, attribute.sort), (SORTED_HEADER, Some(crate::Sort::Descending)));
        }
        Patch::Insert { .. } | Patch::Remove { .. } | Patch::Move { .. } | Patch::Text { .. } | Patch::Value { .. } => {
            panic!("only sort changed")
        }
    }
}
