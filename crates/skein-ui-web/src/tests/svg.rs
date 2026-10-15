use skein_ui::{
    Style,
    scene::{Anchor, Command, Point, Scene},
};

use super::fixtures::{LIMITS, NAMES, OWNER};
use crate::{dom, svg};

#[test]
fn a_scene_draws_as_its_golden_svg() {
    let mut scene = Scene::new(&LIMITS);
    let p = Point { x: 1, y: 2 };
    let q = Point { x: 3, y: 4 };
    assert!(scene.group(p, 6, &[]));
    assert!(scene.line(p, q, &[Style { owner: OWNER, token: 0 }]));
    assert!(scene.hint(b"Line & hint"));
    assert!(scene.act(&[0, 255]));
    assert!(scene.polyline(&[p, q], &[]));
    assert!(scene.path(
        &[Command::Move(p), Command::Line(q), Command::Quadratic(p, q), Command::Cubic(p, q, p), Command::Close],
        &[]
    ));
    assert!(scene.rect(p, q, 2, &[]));
    assert!(scene.circle(q, 3, &[]));
    assert!(scene.text(p, Anchor::Middle, b"Words", &[]));
    assert_eq!(dom::print(&svg::draw(&scene, NAMES, 100, 50)), include_str!("golden/scene.svg").trim_end());
}

#[test]
fn a_refused_scene_draws_nothing() {
    let mut limits = LIMITS;
    limits.scene_marks = 0;
    let mut scene = Scene::new(&limits);
    scene.circle(Point { x: 0, y: 0 }, 1, &[]);
    assert_eq!(dom::print(&svg::draw(&scene, NAMES, 100, 50)), "");
}

#[test]
fn a_marks_text_is_escaped_never_markup() {
    let mut scene = Scene::new(&LIMITS);
    scene.text(Point { x: 0, y: 0 }, Anchor::Start, b"<b>", &[]);
    assert_eq!(
        dom::print(&svg::draw(&scene, NAMES, 10, 20)),
        "<svg aria-hidden=\"true\" width=\"10\" height=\"20\" viewBox=\"0 0 10 20\"><text x=\"0\" y=\"0\" text-anchor=\"start\">&lt;b&gt;</text></svg>"
    );
}
