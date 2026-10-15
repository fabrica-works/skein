use super::fixtures::LIMITS;
use crate::scene::{Anchor, Command, Mark, Point, Scene};
use crate::{Limits, Owner, Style};
const ZERO: Point = Point { x: 0, y: 0 };
#[test]
fn a_mark_past_scene_marks_refuses_the_scene() {
    let limits = Limits { scene_marks: 1, ..LIMITS };
    let mut scene = Scene::new(&limits);
    assert!(scene.rect(ZERO, Point { x: 4, y: 4 }, 0, &[]));
    assert!(!scene.circle(Point { x: 2, y: 2 }, 1, &[]));
    assert_eq!(scene.marks().len(), 0);
}
#[test]
fn bytes_past_scene_bytes_refuse_the_scene() {
    let limits = Limits { scene_bytes: 4, ..LIMITS };
    for kind in 0_u8..6 {
        let mut scene = Scene::new(&limits);
        let accepted = match kind {
            0 => scene.text(ZERO, Anchor::Start, b"12345", &[]),
            1 => scene.polyline(&[ZERO, ZERO], &[]),
            2 => scene.path(&[Command::Move(ZERO)], &[]),
            3 => {
                assert!(scene.circle(ZERO, 1, &[]));
                scene.hint(b"12345")
            }
            4 => {
                assert!(scene.circle(ZERO, 1, &[]));
                scene.act(b"12345")
            }
            5 => scene.scene_act(b"12345"),
            _ => unreachable!(),
        };
        assert!(!accepted, "byte source {kind}");
        assert!(scene.refused(), "byte source {kind}");
        assert_eq!(scene.marks(), &[], "byte source {kind}");
    }
}
#[test]
fn a_refused_scene_keeps_no_mark() {
    let limits = Limits { scene_marks: 1, ..LIMITS };
    let mut scene = Scene::new(&limits);
    assert!(scene.circle(ZERO, 1, &[]));
    assert!(!scene.circle(ZERO, 2, &[]));
    assert!(!scene.line(ZERO, ZERO, &[]));
    assert!(!scene.scene_act(b"act"));
    assert_eq!(scene.marks(), &[]);
    scene.clear();
    assert!(!scene.refused());
    assert!(scene.circle(ZERO, 3, &[]));
    assert_eq!(scene.marks(), &[Mark::Circle { centre: ZERO, radius: 3 }]);
}
#[test]
fn a_group_holds_the_marks_counted_after_it() {
    let mut scene = Scene::new(&LIMITS);
    assert!(scene.group(Point { x: 3, y: 4 }, 2, &[]));
    assert!(scene.line(ZERO, ZERO, &[]));
    assert!(scene.circle(ZERO, 1, &[]));
    assert_eq!(
        scene.marks(),
        &[
            Mark::Group { offset: Point { x: 3, y: 4 }, marks: 2 },
            Mark::Line { from: ZERO, to: ZERO },
            Mark::Circle { centre: ZERO, radius: 1 },
        ]
    );
}
#[test]
fn too_many_tokens_on_a_mark_refuse_the_scene() {
    let limits = Limits { styles_per_node: 1, ..LIMITS };
    let style = Style { owner: Owner { number: 1 }, token: 2 };
    let mut scene = Scene::new(&limits);
    assert!(!scene.circle(ZERO, 1, &[style, style]));
    assert!(scene.refused());
    assert_eq!(scene.marks(), &[]);
}

struct PressCase {
    name: &'static str,
    under: &'static [u32],
    background: bool,
    expected: Option<&'static [u8]>,
}
const PRESSES: &[PressCase] = &[
    PressCase { name: "acting mark under plain mark", under: &[2, 3], background: true, expected: Some(b"mark") },
    PressCase { name: "plain mark in acting group", under: &[4], background: true, expected: Some(b"outer") },
    PressCase { name: "nearest nested acting group", under: &[2], background: true, expected: Some(b"near") },
    PressCase { name: "nothing under point", under: &[], background: true, expected: Some(b"bg\x03\x00\x07\x00") },
    PressCase { name: "no act under point", under: &[5], background: false, expected: None },
];
fn press_scene(background: bool) -> Scene {
    let mut scene = Scene::new(&LIMITS);
    assert!(scene.group(ZERO, 4, &[]));
    assert!(scene.act(b"outer"));
    assert!(scene.group(ZERO, 1, &[]));
    assert!(scene.act(b"near"));
    assert!(scene.circle(ZERO, 1, &[]));
    assert!(scene.line(ZERO, ZERO, &[]));
    assert!(scene.act(b"mark"));
    assert!(scene.circle(ZERO, 2, &[]));
    assert!(scene.circle(ZERO, 3, &[]));
    if background {
        assert!(scene.scene_act(b"bg"));
    }
    scene
}
#[test]
fn a_press_takes_the_act_in_the_designs_order() {
    let mut out = skein_lib::List::with_capacity(16);
    for case in PRESSES {
        let scene = press_scene(case.background);
        let found = scene.act_at(case.under, Point { x: 3, y: 7 }, &mut out);
        assert_eq!(found, case.expected.is_some(), "{}", case.name);
        assert_eq!(out.as_slice(), case.expected.unwrap_or(b""), "{}", case.name);
    }
}
#[test]
fn an_empty_mark_act_still_takes_the_press() {
    let mut scene = Scene::new(&LIMITS);
    assert!(scene.circle(ZERO, 1, &[]));
    assert!(scene.act(b""));
    assert!(scene.scene_act(b"bg"));
    let mut out = skein_lib::List::with_capacity(8);
    assert!(scene.act_at(&[0], ZERO, &mut out));
    assert_eq!(out.as_slice(), b"");
}
#[test]
fn an_act_past_the_output_capacity_is_refused_whole() {
    let scene = press_scene(true);
    let mut out = skein_lib::List::with_capacity(5);
    assert!(!scene.act_at(&[], ZERO, &mut out));
    assert_eq!(out.as_slice(), b"");
}
