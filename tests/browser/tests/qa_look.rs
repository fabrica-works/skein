//! Adversarial look contracts: browser.md, 5.1, 6, 7 and 8.
#![expect(clippy::disallowed_types, reason = "QA of the look's reads")]

use skein_browser::Limits;
use skein_browser::boundary::{Event, Go, Look, Looked, Motion, Rect, Refusal, Request, Scheme};
use skein_browser_world::referee::Referee;
use skein_browser_world::world::{Fault, World};
use skein_lib::{List, Time, Token};

const PAGE: Token = Token::new(3);
const OP: Token = Token::new(40);
const GO: Token = Token::new(41);
const NODE: u64 = 7;

fn opened(limits: Limits) -> World {
    let mut world = World::with_limits(limits);
    world.start();
    world.open();
    world
}

fn properties(names: &[&str]) -> List<Box<[u8]>> {
    let mut list = List::with_capacity(u32::try_from(names.len()).expect("small property list"));
    for name in names {
        list.push(name.as_bytes().into()).expect("reserved property slot");
    }
    list
}

fn answer(world: &mut World, look: Look) -> Event {
    world.ask(Request::Look { page: PAGE, op: OP, look });
    let event = world.above.pop().expect("one look terminal");
    let mut referee = Referee::new();
    referee.look_op(OP, world.env.limits);
    referee.event(&event, 0);
    assert!(world.above.pop().is_none(), "one terminal per operation");
    world.assert_commands_settled();
    event
}

#[test]
fn invalid_look_inputs_refuse_before_sending_commands() {
    let limits = Limits::default();
    let mut too_many = List::with_capacity(limits.properties + 1);
    for _ in 0..=limits.properties {
        too_many.push(Box::from(b"color".as_slice())).expect("reserved property slot");
    }
    let long = vec![b'x'; usize::try_from(limits.text).expect("text fits") + 1].into_boxed_slice();
    let cases = [
        (Look::Style { node: NODE, properties: too_many }, Refusal::Limit),
        (
            Look::Style { node: NODE, properties: properties(&[std::str::from_utf8(&long).expect("ASCII")]) },
            Refusal::Limit,
        ),
        (Look::Select { selector: long }, Refusal::Limit),
        (
            Look::Viewport { width: 0, height: 800, mobile: false, scheme: Scheme::Light, motion: Motion::Reduce },
            Refusal::Protocol,
        ),
        (
            Look::Viewport { width: 400, height: 0, mobile: true, scheme: Scheme::Dark, motion: Motion::NoPreference },
            Refusal::Protocol,
        ),
        (Look::Shot { clip: None, scale: 0 }, Refusal::Protocol),
        (Look::Shot { clip: None, scale: 3 }, Refusal::Protocol),
        (Look::Shot { clip: Some(Rect { x: -2, y: 1000, width: 0, height: 10 }), scale: 2 }, Refusal::Protocol),
        (Look::Shot { clip: Some(Rect { x: 0, y: 1000, width: 10, height: 0 }), scale: 1 }, Refusal::Protocol),
    ];
    for (look, expected) in cases {
        let mut world = opened(limits);
        let before = world.sent.len();
        let event = answer(&mut world, look);
        assert!(matches!(event, Event::Refused { op: OP, why } if why == expected), "{event:?}");
        assert_eq!(world.sent.len(), before, "invalid look sends no command");
    }
}

#[test]
fn tree_follows_children_and_lifts_ignored_ancestors() {
    let mut world = opened(Limits::default());
    world.tree = r#"{"nodes":[
      {"nodeId":"root","role":{"value":"RootWebArea"},"childIds":["ignored","tail"]},
      {"nodeId":"tail","backendDOMNodeId":14,"role":{"value":"heading"},"name":{"value":"Last"},"properties":[{"name":"level","value":{"value":3}}]},
      {"nodeId":"deep","backendDOMNodeId":12,"role":{"value":"heading"},"name":{"value":"First"},"properties":[{"name":"level","value":{"value":2}}]},
      {"nodeId":"ignored","ignored":true,"childIds":["ignored-too"]},
      {"nodeId":"ignored-too","ignored":true,"childIds":["deep"]}
    ]}"#.into();
    let event = answer(&mut world, Look::Tree);
    let Event::Looked { looked: Looked::Tree(entries), .. } = event else { panic!("{event:?}") };
    let entries: Vec<_> =
        entries.iter().map(|entry| (entry.node, entry.depth, entry.name.as_ref(), entry.level)).collect();
    assert_eq!(
        entries,
        vec![(None, 0, b"".as_slice(), 0), (Some(12), 1, b"First".as_slice(), 2), (Some(14), 1, b"Last".as_slice(), 3)]
    );
}

#[test]
fn tree_entry_bound_counts_visible_entries_without_cutting() {
    // The fake's tree has two visible entries and one ignored ancestor.
    for (entries, accepted) in [(2, true), (1, false)] {
        let mut world = opened(Limits { entries, ..Limits::default() });
        let event = answer(&mut world, Look::Tree);
        if accepted {
            assert!(
                matches!(event, Event::Looked { looked: Looked::Tree(ref tree), .. } if tree.len() == 2),
                "{event:?}"
            );
        } else {
            assert!(matches!(event, Event::Refused { why: Refusal::Limit, .. }), "{event:?}");
        }
    }
}

#[test]
fn style_keeps_requested_order_repeats_and_unknown_properties() {
    let mut world = opened(Limits::default());
    let event = answer(
        &mut world,
        Look::Style { node: NODE, properties: properties(&["color", "--absent", "font-size", "color"]) },
    );
    let Event::Looked { looked: Looked::Style { values, .. }, .. } = event else { panic!("{event:?}") };
    let values: Vec<_> = values.iter().map(|property| (property.name.as_ref(), property.value.as_ref())).collect();
    assert_eq!(
        values,
        vec![
            (b"color".as_slice(), b"rgb(17, 17, 17)".as_slice()),
            (b"--absent".as_slice(), b"".as_slice()),
            (b"font-size".as_slice(), b"16px".as_slice()),
            (b"color".as_slice(), b"rgb(17, 17, 17)".as_slice())
        ]
    );
}

#[test]
fn computed_value_bound_returns_whole_value_or_limit() {
    let limits = Limits::default();
    for (length, accepted) in [(limits.text, true), (limits.text + 1, false)] {
        let value = "x".repeat(usize::try_from(length).expect("text fits"));
        let mut world = opened(limits);
        world.styles = format!(r#"{{"computedStyle":[{{"name":"color","value":"{value}"}}]}}"#);
        let event = answer(&mut world, Look::Style { node: NODE, properties: properties(&["color"]) });
        if accepted {
            let Event::Looked { looked: Looked::Style { values, .. }, .. } = event else { panic!("{event:?}") };
            assert_eq!(
                values.iter().map(|property| property.value.as_ref()).collect::<Vec<_>>(),
                vec![value.as_bytes()]
            );
        } else {
            assert!(matches!(event, Event::Refused { why: Refusal::Limit, .. }), "{event:?}");
        }
    }
}

#[test]
fn style_face_bound_refuses_instead_of_truncating() {
    // The fake names two faces, one custom and one fallback.
    for (entries, accepted) in [(2, true), (1, false)] {
        let mut world = opened(Limits { entries, ..Limits::default() });
        let event = answer(&mut world, Look::Style { node: NODE, properties: properties(&[]) });
        if accepted {
            assert!(
                matches!(event, Event::Looked { looked: Looked::Style { ref faces, .. }, .. } if faces.len() == 2),
                "{event:?}"
            );
        } else {
            assert!(matches!(event, Event::Refused { why: Refusal::Limit, .. }), "{event:?}");
        }
    }
}

#[test]
fn style_without_a_box_keeps_values_and_faces() {
    let mut world = opened(Limits::default());
    world.fault(b"DOM.getBoxModel", Fault::NoBox);
    let event = answer(&mut world, Look::Style { node: NODE, properties: properties(&["color"]) });
    assert!(
        matches!(event, Event::Looked { looked: Looked::Style { rect: None, ref values, ref faces }, .. } if values.len() == 1 && faces.len() == 2),
        "{event:?}"
    );
}

#[test]
fn style_removed_between_commands_is_gone() {
    let mut world = opened(Limits::default());
    world.fault(b"CSS.getPlatformFontsForNode", Fault::Vanish);
    let event = answer(&mut world, Look::Style { node: NODE, properties: properties(&["color"]) });
    assert!(matches!(event, Event::Refused { why: Refusal::Gone, .. }), "{event:?}");
}

#[test]
fn screenshot_bound_applies_to_decoded_png_bytes() {
    // The world's PNG fixture decodes to its eight-byte signature.
    for (screenshot, accepted) in [(8, true), (7, false)] {
        let mut world = opened(Limits { screenshot, ..Limits::default() });
        let event =
            answer(&mut world, Look::Shot { clip: Some(Rect { x: 10, y: 2000, width: 30, height: 20 }), scale: 2 });
        if accepted {
            assert!(
                matches!(event, Event::Looked { looked: Looked::Shot(ref png), .. } if png.as_ref() == b"\x89PNG\r\n\x1a\n"),
                "{event:?}"
            );
        } else {
            assert!(matches!(event, Event::Refused { why: Refusal::Limit, .. }), "{event:?}");
        }
    }
}

#[test]
fn every_look_command_times_out_and_late_reply_is_harmless() {
    let cases = [
        (b"Accessibility.getFullAXTree".as_slice(), Look::Tree),
        (b"DOM.querySelector".as_slice(), Look::Select { selector: b"#part".as_slice().into() }),
        (b"CSS.getComputedStyleForNode".as_slice(), Look::Style { node: NODE, properties: properties(&["color"]) }),
        (b"DOM.requestChildNodes".as_slice(), Look::Style { node: NODE, properties: properties(&[]) }),
        (b"CSS.getPlatformFontsForNode".as_slice(), Look::Style { node: NODE, properties: properties(&[]) }),
        (b"DOM.getBoxModel".as_slice(), Look::Style { node: NODE, properties: properties(&[]) }),
        (b"Page.captureScreenshot".as_slice(), Look::Shot { clip: None, scale: 1 }),
        (
            b"Emulation.setEmulatedMedia".as_slice(),
            Look::Viewport { width: 400, height: 800, mobile: true, scheme: Scheme::Dark, motion: Motion::Reduce },
        ),
    ];
    for (method, look) in cases {
        let mut world = opened(Limits::default());
        world.fault(method, Fault::Delay);
        world.ask(Request::Look { page: PAGE, op: OP, look });
        assert!(world.above.pop().is_none(), "{method:?} still pending");
        world.tick(Time::ZERO.saturating_add(world.env.limits.answer));
        let event = world.above.pop().expect("deadline terminal");
        assert!(matches!(event, Event::Refused { op: OP, why: Refusal::Timeout }), "{method:?}: {event:?}");
        world.release_delayed();
        assert!(world.above.pop().is_none(), "late answer creates no second terminal");
        world.assert_commands_settled();
    }
}

#[test]
fn tree_restarts_when_a_go_overtakes_its_reply() {
    let mut world = opened(Limits::default());
    world.fault(b"Accessibility.getFullAXTree", Fault::Delay);
    world.ask(Request::Look { page: PAGE, op: OP, look: Look::Tree });
    world.tree =
        r#"{"nodes":[{"nodeId":"new","role":{"value":"RootWebArea"},"name":{"value":"New document"}}]}"#.into();
    world.ask(Request::Go { page: PAGE, op: GO, to: Go::Reload });
    world.release_delayed();
    // The restarted read asks again at the loop's next turn, once the go has committed.
    let now = world.env.now;
    world.tick(now);
    let mut terminals = Vec::new();
    while let Some(event) = world.above.pop() {
        match event {
            Event::Done { op: GO } => {}
            Event::Looked { op: OP, looked: Looked::Tree(entries) } => {
                terminals.push(entries.iter().map(|entry| entry.name.to_vec()).collect::<Vec<_>>())
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert_eq!(terminals, vec![vec![b"New document".to_vec()]], "look reads only the committed document");
    world.assert_commands_settled();
}

#[test]
fn select_restarts_when_a_go_overtakes_its_reply() {
    let mut world = opened(Limits::default());
    world.fault(b"DOM.querySelector", Fault::Delay);
    world.ask(Request::Look { page: PAGE, op: OP, look: Look::Select { selector: b"#part".as_slice().into() } });
    world.ask(Request::Go { page: PAGE, op: GO, to: Go::Reload });
    world.release_delayed();
    // The restarted read asks again at the loop's next turn, once the go has committed.
    let now = world.env.now;
    world.tick(now);
    let mut terminals = Vec::new();
    while let Some(event) = world.above.pop() {
        match event {
            Event::Done { op: GO } => {}
            Event::Looked { op: OP, looked: Looked::Selected(node) } => terminals.push(node),
            other => panic!("unexpected event: {other:?}"),
        }
    }
    // The fake describes every node as backend 7, so the restart shows in the commands: the
    // selector is asked again of the committed document, after the reload.
    assert_eq!(terminals, vec![Some(7)]);
    let reload = world.sent.iter().position(|sent| contains(sent, b"Page.reload")).expect("the go reloads");
    assert!(
        world.sent[reload..].iter().any(|sent| contains(sent, b"DOM.querySelector")),
        "select reads only the committed document"
    );
    world.assert_commands_settled();
}

#[test]
fn style_restarts_when_a_go_removes_its_node() {
    let mut world = opened(Limits::default());
    world.fault(b"CSS.getComputedStyleForNode", Fault::Delay);
    world.ask(Request::Look {
        page: PAGE,
        op: OP,
        look: Look::Style { node: NODE, properties: properties(&["color"]) },
    });
    world.gone = true;
    world.ask(Request::Go { page: PAGE, op: GO, to: Go::Reload });
    world.release_delayed();
    // The restarted read asks again at the loop's next turn, once the go has committed.
    let now = world.env.now;
    world.tick(now);
    let mut terminals = Vec::new();
    while let Some(event) = world.above.pop() {
        match event {
            Event::Done { op: GO } => {}
            Event::Refused { op: OP, why } => terminals.push(why),
            other => panic!("style of an earlier document must be Gone: {other:?}"),
        }
    }
    assert_eq!(terminals, vec![Refusal::Gone]);
    world.assert_commands_settled();
}

#[test]
fn exact_property_and_selector_input_limits_are_admitted() {
    let limits = Limits::default();
    let text = "x".repeat(usize::try_from(limits.text).expect("text fits"));
    let mut names = List::with_capacity(limits.properties);
    for _ in 0..limits.properties {
        names.push(Box::from(b"color".as_slice())).expect("reserved property slot");
    }
    let cases = [
        Look::Style { node: NODE, properties: names },
        Look::Style { node: NODE, properties: properties(&[&text]) },
        Look::Select { selector: text.into_bytes().into_boxed_slice() },
    ];
    for look in cases {
        let mut world = opened(limits);
        let event = answer(&mut world, look);
        assert!(matches!(event, Event::Looked { .. }), "at the input limit: {event:?}");
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

fn faces(world: &mut World) -> Vec<(String, bool, u32)> {
    let event = answer(world, Look::Style { node: NODE, properties: properties(&[]) });
    let Event::Looked { looked: Looked::Style { faces, .. }, .. } = event else { panic!("{event:?}") };
    let mut faces: Vec<_> = faces
        .iter()
        .map(|face| (String::from_utf8(face.family.to_vec()).expect("font family is UTF-8"), face.custom, face.glyphs))
        .collect();
    faces.sort();
    faces
}

#[test]
fn faces_include_text_beneath_nested_elements_and_ignore_empty_elements() {
    let mut world = opened(Limits::default());
    world.children = r#"[
      {"nodeId":8,"nodeType":3,"nodeValue":"Direct"},
      {"nodeId":9,"nodeType":1,"children":[
        {"nodeId":10,"nodeType":1,"children":[{"nodeId":11,"nodeType":3,"nodeValue":"Deep"}]},
        {"nodeId":12,"nodeType":1,"children":[]}]},
      {"nodeId":13,"nodeType":8,"nodeValue":"Comment"}]
    "#
    .into();
    world.fonts.clear();
    world.fonts.insert(8, r#"{"fonts":[{"familyName":"Direct face","isCustomFont":false,"glyphCount":6}]}"#.into());
    world.fonts.insert(11, r#"{"fonts":[{"familyName":"Deep face","isCustomFont":true,"glyphCount":4}]}"#.into());
    world.fonts.insert(12, r#"{"fonts":[{"familyName":"Unused","isCustomFont":false,"glyphCount":99}]}"#.into());
    assert_eq!(faces(&mut world), vec![("Deep face".into(), true, 4), ("Direct face".into(), false, 6)]);
}

#[test]
fn faces_without_text_have_no_used_fonts() {
    let mut world = opened(Limits::default());
    world.children = r#"[{"nodeId":8,"nodeType":1,"children":[]}]"#.into();
    assert_eq!(faces(&mut world), vec![]);
}

#[test]
fn faces_merge_the_same_family_and_custom_flag_across_text_nodes() {
    let mut world = opened(Limits { entries: 1, ..Limits::default() });
    world.children = r#"[{"nodeId":8,"nodeType":3,"nodeValue":"One"},
                         {"nodeId":9,"nodeType":1,"children":[{"nodeId":10,"nodeType":3,"nodeValue":"Two"}]}]"#
        .into();
    world.fonts.clear();
    for (node, glyphs) in [(8, 3), (10, 4)] {
        world.fonts.insert(
            node,
            format!(r#"{{"fonts":[{{"familyName":"Shared","isCustomFont":true,"glyphCount":{glyphs}}}]}}"#),
        );
    }
    assert_eq!(faces(&mut world), vec![("Shared".into(), true, 7)]);
}

#[test]
fn faces_keep_web_and_platform_fonts_with_the_same_family_distinct() {
    let mut world = opened(Limits::default());
    world.fonts.insert(
        8,
        r#"{"fonts":[
      {"familyName":"Shared","isCustomFont":true,"glyphCount":3},
      {"familyName":"Shared","isCustomFont":false,"glyphCount":2}]}"#
            .into(),
    );
    assert_eq!(faces(&mut world), vec![("Shared".into(), false, 2), ("Shared".into(), true, 3)]);
}

#[test]
fn faces_limit_counts_distinct_faces_across_all_descendants() {
    for (entries, accepted) in [(2, true), (1, false)] {
        let mut world = opened(Limits { entries, ..Limits::default() });
        world.children = r#"[{"nodeId":8,"nodeType":3,"nodeValue":"One"},
                             {"nodeId":9,"nodeType":1,"children":[{"nodeId":10,"nodeType":3,"nodeValue":"Two"}]}]"#
            .into();
        world.fonts.clear();
        for (node, family) in [(8, "One"), (10, "Two")] {
            world.fonts.insert(
                node,
                format!(r#"{{"fonts":[{{"familyName":"{family}","isCustomFont":false,"glyphCount":3}}]}}"#),
            );
        }
        let event = answer(&mut world, Look::Style { node: NODE, properties: properties(&[]) });
        if accepted {
            let Event::Looked { looked: Looked::Style { faces, .. }, .. } = event else { panic!("{event:?}") };
            assert_eq!(faces.len(), 2);
        } else {
            assert!(matches!(event, Event::Refused { why: Refusal::Limit, .. }), "{event:?}");
        }
    }
}

#[test]
fn faces_read_does_not_keep_the_previous_nodes_fonts() {
    let mut world = opened(Limits::default());
    assert_eq!(faces(&mut world).len(), 2);
    world.children = "[]".into();
    world.ask(Request::Look {
        page: PAGE,
        op: Token::new(42),
        look: Look::Style { node: NODE, properties: properties(&[]) },
    });
    let event = world.above.pop().expect("second style terminal");
    let Event::Looked { op, looked: Looked::Style { faces, .. } } = event else { panic!("{event:?}") };
    assert_eq!(op, Token::new(42));
    assert_eq!(faces.len(), 0);
    assert!(world.above.pop().is_none());
    world.assert_commands_settled();
}

#[test]
fn faces_glyph_sum_past_u32_is_refused_instead_of_wrapping() {
    let mut world = opened(Limits::default());
    world.children = r#"[{"nodeId":8,"nodeType":3,"nodeValue":"One"},
                         {"nodeId":9,"nodeType":3,"nodeValue":"Two"}]"#
        .into();
    for (node, glyphs) in [(8, u32::MAX), (9, 1)] {
        world.fonts.insert(
            node,
            format!(r#"{{"fonts":[{{"familyName":"Shared","isCustomFont":false,"glyphCount":{glyphs}}}]}}"#),
        );
    }
    let event = answer(&mut world, Look::Style { node: NODE, properties: properties(&[]) });
    assert!(
        matches!(event, Event::Refused { op: OP, .. }),
        "overflow must not return a wrapped glyph count: {event:?}"
    );
}

#[test]
fn faces_pending_read_close_cancels_once_and_late_reply_adds_nothing() {
    let mut world = opened(Limits::default());
    world.fault(b"CSS.getPlatformFontsForNode", Fault::Delay);
    world.ask(Request::Look { page: PAGE, op: OP, look: Look::Style { node: NODE, properties: properties(&[]) } });
    assert!(world.above.pop().is_none());
    world.ask(Request::Close { entity: PAGE });
    let mut referee = Referee::new();
    referee.look_op(OP, world.env.limits);
    let mut refused = 0;
    let mut closed = 0;
    while let Some(event) = world.above.pop() {
        referee.event(&event, 0);
        match event {
            Event::Refused { op: OP, .. } => refused += 1,
            Event::Closed { owner: PAGE } => closed += 1,
            other => panic!("close cancels style once: {other:?}"),
        }
    }
    assert_eq!((refused, closed), (1, 1));
    world.release_delayed();
    assert!(world.above.pop().is_none());
    world.assert_commands_settled();
}
