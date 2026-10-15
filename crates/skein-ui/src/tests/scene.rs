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
