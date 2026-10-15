use crate::{hidden::Hidden, transport::StreamDuty};
use skein_lib::{Duration, Time};

#[test]
fn the_stream_goes_a_minute_after_the_tab_hides() {
    let mut hidden = Hidden::new(Duration::from_secs(60));
    hidden.visibility(true, Time::ZERO);
    let before = hidden.fire(Time::ZERO.saturating_add(Duration::from_secs(59)));
    let due = hidden.fire(Time::ZERO.saturating_add(Duration::from_secs(60)));
    assert_eq!((before, due), (None, Some(StreamDuty::Close)));
}
#[test]
fn an_opening_while_hidden_waits_for_shown() {
    let mut hidden = Hidden::new(Duration::from_secs(60));
    hidden.visibility(true, Time::ZERO);
    let held = hidden.carry(StreamDuty::Open { path: b"/events".as_slice().into(), headers: vec![], bound: 16 });
    let shown = hidden.visibility(false, Time::ZERO);
    assert_eq!(
        (held, shown),
        (None, Some(StreamDuty::Open { path: b"/events".as_slice().into(), headers: vec![], bound: 16 }))
    );
}
