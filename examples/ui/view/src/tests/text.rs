//! The real text shell over the example pages (examples.md, 9.5).
use super::fixtures;
use crate::{State, limits};
use skein_lib::Queue;
use skein_ui::View;
use skein_ui_text::{Print, render};
fn print(state: &State) -> Print {
    let limits = limits::limits();
    let mut view = View::new(&limits);
    state.build(&mut view, &mut Queue::with_capacity(limits.patches));
    let mut print = Print::new(&limits).expect("text shell fits the view limits");
    render(view.tree(), b"Notes", None, &mut print);
    print
}
#[test]
fn the_list_prints_as_its_golden_text() {
    let print = print(&fixtures::state());
    assert_eq!(
        print.text.as_slice(),
        std::fs::read("src/tests/golden/list.txt").expect("committed list golden").as_slice()
    );
}
#[test]
fn the_empty_page_prints_its_words() {
    let print = print(&State::start(1, b"/", &[], 0, b"en"));
    assert_eq!(
        print.text.as_slice(),
        std::fs::read("src/tests/golden/empty.txt").expect("committed empty golden").as_slice()
    );
}
