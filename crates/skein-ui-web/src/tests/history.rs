use crate::history::History;
#[test]
fn a_place_gone_to_is_reported_as_a_path() {
    let mut history = History::default();
    assert_eq!(history.gone_to(b"/notes/7?q=a", (30.0, 80.0)).as_ref(), b"/notes/7?q=a");
}
#[test]
fn an_entrys_scroll_is_restored_once() {
    let mut history = History::default();
    let _path = history.gone_to(b"/notes/7", (30.0, 80.0));
    assert_eq!(history.take_scroll(), Some((30.0, 80.0)));
    assert_eq!(history.take_scroll(), None);
}
#[test]
fn the_title_joins_the_heading_and_application_name() {
    for (heading, expected) in [(b"Groceries".as_slice(), "Groceries · Notes"), (b"", "Notes")] {
        assert_eq!(crate::title::text(heading, b"Notes"), expected);
    }
}
#[test]
fn the_timer_reports_its_deadline_once() {
    let mut timer = crate::timer::Timer::default();
    timer.set(Some(skein_lib::Time::from_nanos(3)));
    assert!(!timer.due(skein_lib::Time::from_nanos(2)));
    assert!(timer.due(skein_lib::Time::from_nanos(3)));
    assert!(!timer.due(skein_lib::Time::from_nanos(3)));
}
