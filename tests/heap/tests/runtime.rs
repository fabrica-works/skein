//! The runtime's own heap is never a step's (skein-heap's "The runtime's own").

use core::cell::Cell;
use skein_heap::{Counting, Meter, Span};

#[global_allocator]
static HEAP: Counting = Counting;

std::thread_local! {
    static FIRST: Cell<Option<Box<u8>>> = const { Cell::new(None) };
    static SECOND: Cell<Option<Box<u8>>> = const { Cell::new(None) };
}

#[test]
fn a_threads_first_destructor_record_is_neither_a_spans_nor_a_steps() {
    let span = Span::start();
    FIRST.with(|first| first.set(None));
    let grown = span.end();
    assert_eq!((grown.peak, grown.net), (0, 0), "a span over the thread's first thread-local with a destructor");
    let meter = Meter::new();
    meter.start();
    SECOND.with(|second| second.set(None));
    let step = meter.end();
    assert_eq!((step.peak(), step.held()), (0, 0), "a step over another");
}

#[test]
fn the_first_write_to_standard_error_is_neither_a_spans_nor_a_steps() {
    let span = Span::start();
    eprintln!("a span's first line to the standard error");
    let grown = span.end();
    assert_eq!((grown.peak, grown.net), (0, 0), "a span over the first write to the standard error");
    let meter = Meter::new();
    meter.start();
    eprintln!("a step's line to the standard error");
    let step = meter.end();
    assert_eq!((step.peak(), step.held()), (0, 0), "a step over another");
}
