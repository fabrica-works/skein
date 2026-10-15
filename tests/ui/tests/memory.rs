//! UI reservations and full pages measured on the counting allocator.
use skein_heap::{Counting, Meter};
use skein_lib::Queue;
use skein_ui::scene::{Anchor, Point, Scene};
use skein_ui::{Builder, Limits, Owner, Role, Style, View, markdown, worst_case};
use skein_ui_tests::full_page;

#[global_allocator]
static HEAP: Counting = Counting;
const LIMITS: Limits = Limits {
    nodes: 32,
    depth: 8,
    text_per_node: 8,
    text_bytes: 256,
    drawing_bytes: 0,
    styles_per_node: 2,
    patches: 64,
    bindings: 0,
    slots: 0,
    markdown_depth: 2,
    scene_marks: 32,
    scene_bytes: 256,
};
fn full_scene(limits: &Limits) -> Scene {
    let mut scene = Scene::new(limits);
    let tokens = [Style { owner: Owner { number: 1 }, token: 1 }, Style { owner: Owner { number: 1 }, token: 2 }];
    for _ in 0..limits.scene_marks {
        assert!(scene.text(Point { x: 0, y: 0 }, Anchor::Start, b"abcdefgh", &tokens));
    }
    scene
}
fn peak() -> (u64, u64) {
    let bound = worst_case(&LIMITS).expect("fixture capacities fit");
    let meter = Meter::new();
    meter.start();
    let mut builder = Builder::new(&LIMITS);
    full_page(&LIMITS, &mut builder);
    let mut view = View::new(&LIMITS);
    let mut patches = Queue::with_capacity(LIMITS.patches);
    let scene = full_scene(&LIMITS);
    let mut first = view.builder();
    full_page(&LIMITS, &mut first);
    view.finish(first, 1, &mut patches);
    while patches.pop().is_some() {}
    let mut second = view.builder();
    full_page(&LIMITS, &mut second);
    view.finish(second, 2, &mut patches);
    let mut words = Builder::new(&LIMITS);
    words.open(Role::Page);
    markdown::read(b"- x\n  - y\n", 0, &mut words);
    words.close();
    let words = words.finish();
    let measured = meter.end();
    drop((words, scene, patches, view, builder));
    (measured.peak(), bound)
}
#[test]
fn the_view_holds_no_more_than_its_worst_case() {
    let (peak, bound) = peak();
    assert!(peak <= bound, "full pages, diff, Markdown and scene: {peak} <= {bound}");
}
#[test]
fn a_full_page_reaches_half_its_worst_case() {
    let (peak, bound) = peak();
    assert!(peak >= bound / 2, "full pages reach the limits: {peak} >= {bound}/2");
}
