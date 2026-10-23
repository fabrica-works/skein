//! The runtime's own heap is never a step's (skein-heap's "The runtime's own").

use core::cell::Cell;
use skein_heap::{Counting, Meter, Span};

#[global_allocator]
static HEAP: Counting = Counting;

/// Makes thread-locals with a destructor, and a function that reaches each
/// for the first time on its thread.
macro_rules! records {
    ($reach:ident: $($record:ident)*) => {
        std::thread_local! {
            $(static $record: Cell<Option<Box<u8>>> = const { Cell::new(None) };)*
        }

        fn $reach() {
            $($record.with(|record| record.set(None));)*
        }
    };
}

// Sixteen each, more than any world reaches for the first time in a step.
records!(reach_in_a_span: S0 S1 S2 S3 S4 S5 S6 S7 S8 S9 S10 S11 S12 S13 S14 S15);
records!(reach_in_a_step: T0 T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15);

#[test]
fn a_threads_destructor_record_is_neither_a_spans_nor_a_steps() {
    let span = Span::start();
    reach_in_a_span();
    let grown = span.end();
    assert_eq!(
        (grown.peak, grown.net),
        (0, 0),
        "a span over 16 new thread-locals with a destructor: std's list grew past the room skein-heap's 33 leave"
    );
    let meter = Meter::new();
    meter.start();
    reach_in_a_step();
    let step = meter.end();
    assert_eq!((step.peak(), step.held()), (0, 0), "a step over 16 more");
}
