//! Directory requests retain the owner's two roots (io.md, 5).
use super::file::complete;
use crate::{
    file::{Event, Request},
    file_layer::{self, FileIo},
    kernel::{Done, Error, Fd, Op},
};
use alloc::boxed::Box;
use skein_lib::{Duration, Queue, Time, Token};
const OWNER: Token = Token::new(7);
const STAGING: Fd = Fd::new(41);
const CACHE: Fd = Fd::new(42);

#[test]
fn a_rename_names_the_directory_it_leaves_and_the_one_it_lands_in() {
    let mut io = FileIo::new(3, 16, 4, Duration::from_secs(1));
    let (mut events, mut subs) = (Queue::with_capacity(2), Queue::with_capacity(2));
    let (from, to): (Box<[u8]>, Box<[u8]>) = (Box::from(&b"InRelease"[..]), Box::from(&b"index"[..]));
    file_layer::down(
        &mut io,
        Time::ZERO,
        Request::Rename { owner: OWNER, from_root: STAGING, from: from.clone(), to_root: CACHE, to: to.clone() },
        &mut events,
        &mut subs,
    );
    let submit = subs.pop().expect("one rename");
    assert_eq!(submit.kind, Op::Rename { from_dir: STAGING, from, to_dir: CACHE, to });
    complete(&mut io, submit, Ok(Done::Nothing), &mut events, &mut subs);
    assert_eq!(events.pop(), Some(Event::Renamed { owner: OWNER }));
}

#[test]
fn a_rename_of_a_name_holding_a_slash_fails_before_any_record() {
    for (from, to) in [(b"dir/from".as_slice(), b"to".as_slice()), (b"from".as_slice(), b"dir/to".as_slice())] {
        let mut io = FileIo::new(3, 16, 4, Duration::from_secs(1));
        let (mut events, mut subs) = (Queue::with_capacity(2), Queue::with_capacity(2));
        file_layer::down(
            &mut io,
            Time::ZERO,
            Request::Rename {
                owner: OWNER,
                from_root: STAGING,
                from: Box::from(from),
                to_root: CACHE,
                to: Box::from(to),
            },
            &mut events,
            &mut subs,
        );
        assert_eq!(
            events.pop(),
            Some(Event::Failed { owner: OWNER, error: Error::InvalidArgument, committed: false, residue: None })
        );
        assert!(subs.is_empty());
    }
}

use super::file::listed;
use crate::file::Entry;
use crate::kernel::{Kind, Submit};
struct Page {
    io: FileIo,
    directory: Token,
    events: Queue<Event>,
    subs: Queue<Submit>,
}
impl Page {
    fn new() -> Self {
        let mut io = FileIo::new(3, 16, 4, Duration::from_secs(1));
        let directory = io.adopt_root(CACHE).expect("one directory");
        Self { io, directory, events: Queue::with_capacity(2), subs: Queue::with_capacity(2) }
    }
    fn start(&mut self, max: u32) {
        file_layer::down(
            &mut self.io,
            Time::ZERO,
            Request::ListPage { owner: OWNER, directory: self.directory, max },
            &mut self.events,
            &mut self.subs,
        );
    }
    fn answer(&mut self, names: &[&[u8]]) {
        let submit = self.subs.pop().expect("a list in flight");
        listed(&mut self.io, submit, names, &mut self.events, &mut self.subs);
    }
    fn failed(&mut self, error: Error) {
        assert_eq!(self.events.pop(), Some(Event::Failed { owner: OWNER, error, committed: false, residue: None }));
        assert!(self.events.is_empty() && self.subs.is_empty() && self.io.takes());
        assert_eq!(self.io.descriptor(self.directory), Some(CACHE));
    }
}

#[test]
fn a_page_fills_to_max_across_short_kernel_lists() {
    let mut page = Page::new();
    page.start(3);
    page.answer(&[b"a"]);
    let Op::List { entries, .. } = &page.subs.iter().next().expect("next list").kind else { panic!("list") };
    assert_eq!(entries.len(), 2);
    page.answer(&[b"b", b"c"]);
    let entries: Box<[Entry]> = Box::new([
        Entry { name: Box::from(&b"a"[..]), kind: Kind::File },
        Entry { name: Box::from(&b"b"[..]), kind: Kind::File },
        Entry { name: Box::from(&b"c"[..]), kind: Kind::File },
    ]);
    assert_eq!(page.events.pop(), Some(Event::Listed { owner: OWNER, entries }));
    assert!(page.subs.is_empty() && page.io.takes());
    assert_eq!(page.io.descriptor(page.directory), Some(CACHE));
}

#[test]
fn a_page_shorter_than_max_is_the_directorys_end() {
    let mut page = Page::new();
    page.start(4);
    page.answer(&[b"last"]);
    assert!(page.events.is_empty());
    page.answer(&[]);
    let entries: Box<[Entry]> = Box::new([Entry { name: Box::from(&b"last"[..]), kind: Kind::File }]);
    assert_eq!(page.events.pop(), Some(Event::Listed { owner: OWNER, entries }));
    assert!(page.subs.is_empty() && page.io.takes());
}

#[test]
fn a_page_after_the_end_is_empty() {
    let mut page = Page::new();
    for _ in 0..2_u32 {
        page.start(4);
        page.answer(&[]);
        assert_eq!(page.events.pop(), Some(Event::Listed { owner: OWNER, entries: Box::new([]) }));
        assert!(page.subs.is_empty() && page.io.takes());
    }
}

#[test]
fn a_page_past_max_entries_fails_before_any_record() {
    let mut page = Page::new();
    page.start(5);
    page.failed(Error::InvalidArgument);
}
#[test]
fn a_page_of_an_unknown_token_fails_not_found() {
    let mut page = Page::new();
    let directory = page.directory;
    page.directory = Token::new(99);
    page.start(4);
    page.directory = directory;
    page.failed(Error::NotFound);
}
#[test]
fn a_page_of_zero_fails_invalid_argument() {
    let mut page = Page::new();
    page.start(0);
    page.failed(Error::InvalidArgument);
}
#[test]
fn a_page_cut_short_by_a_deadline_fails_timed_out() {
    let mut page = Page::new();
    page.start(3);
    file_layer::expire(&mut page.io, Time::ZERO.saturating_add(Duration::from_secs(2)), &mut page.subs);
    assert_eq!(page.subs.len(), 1, "no cancel for a synchronous list");
    page.answer(&[b"lost"]);
    page.failed(Error::TimedOut);
}
#[test]
fn a_page_cut_short_by_a_cancel_is_cancelled() {
    let mut page = Page::new();
    page.start(3);
    file_layer::cancel(&mut page.io, OWNER, &mut page.subs);
    assert_eq!(page.subs.len(), 1, "no cancel for a synchronous list");
    page.answer(&[b"lost"]);
    assert_eq!(page.events.pop(), Some(Event::Cancelled { owner: OWNER }));
    assert!(page.subs.is_empty() && page.io.takes());
    assert_eq!(page.io.descriptor(page.directory), Some(CACHE));
}
#[test]
fn a_page_the_list_in_flight_finishes_is_listed_past_its_deadline() {
    for names in [&[b"last".as_slice()][..], &[][..]] {
        let mut page = Page::new();
        page.start(1);
        file_layer::expire(&mut page.io, Time::ZERO.saturating_add(Duration::from_secs(2)), &mut page.subs);
        page.answer(names);
        let entries: Box<[Entry]> = if names.is_empty() {
            Box::new([])
        } else {
            Box::new([Entry { name: Box::from(&b"last"[..]), kind: Kind::File }])
        };
        assert_eq!(page.events.pop(), Some(Event::Listed { owner: OWNER, entries }));
        assert!(page.subs.is_empty() && page.io.takes());
    }
}

impl Page {
    fn lock(&mut self) {
        file_layer::down(
            &mut self.io,
            Time::ZERO,
            Request::Lock { owner: OWNER, root: self.directory },
            &mut self.events,
            &mut self.subs,
        );
        assert_eq!(self.io.next_deadline(), None, "a lock has no timer");
    }
    fn lock_answer(&mut self, result: Result<Done, Error>) {
        let submit = self.subs.pop().expect("one lock");
        assert_eq!(submit.kind, Op::Lock { fd: CACHE });
        complete(&mut self.io, submit, result, &mut self.events, &mut self.subs);
        assert!(self.subs.is_empty() && self.io.takes());
        assert_eq!(self.io.next_deadline(), None);
        assert_eq!(self.io.descriptor(self.directory), Some(CACHE));
    }
}
#[test]
fn a_lock_asks_the_kernel_lock_and_tells_locked() {
    let mut page = Page::new();
    page.lock();
    page.lock_answer(Ok(Done::Nothing));
    assert_eq!(page.events.pop(), Some(Event::Locked { owner: OWNER }));
}
#[test]
fn a_busy_answer_tells_the_owner_busy_and_arms_no_deadline() {
    let mut page = Page::new();
    page.lock();
    page.lock_answer(Err(Error::Busy));
    assert_eq!(page.events.pop(), Some(Event::Busy { owner: OWNER }));
}
#[test]
fn another_kernel_error_fails_the_lock_with_it() {
    let mut page = Page::new();
    page.lock();
    page.lock_answer(Err(Error::NoBufferSpace));
    page.failed(Error::NoBufferSpace);
}
#[test]
fn a_lock_of_an_unknown_token_fails_not_found() {
    let mut page = Page::new();
    let directory = page.directory;
    page.directory = Token::new(99);
    page.lock();
    page.directory = directory;
    page.failed(Error::NotFound);
    assert_eq!(page.io.next_deadline(), None);
}
#[test]
fn an_owners_cancel_of_a_lock_waits_for_its_own_answer() {
    for (result, expected) in
        [(Ok(Done::Nothing), Event::Locked { owner: OWNER }), (Err(Error::Busy), Event::Busy { owner: OWNER })]
    {
        let mut page = Page::new();
        page.lock();
        file_layer::cancel(&mut page.io, OWNER, &mut page.subs);
        assert_eq!(page.subs.len(), 1, "no lock cancel");
        assert!(page.events.is_empty());
        page.lock_answer(result);
        assert_eq!(page.events.pop(), Some(expected));
    }
}
#[test]
fn a_lock_answered_after_its_deadline_keeps_its_own_answer() {
    for (result, expected) in
        [(Ok(Done::Nothing), Event::Locked { owner: OWNER }), (Err(Error::Busy), Event::Busy { owner: OWNER })]
    {
        let mut page = Page::new();
        page.lock();
        file_layer::expire(&mut page.io, Time::ZERO.saturating_add(Duration::from_secs(2)), &mut page.subs);
        assert_eq!(page.subs.len(), 1, "no lock cancel");
        page.lock_answer(result);
        assert_eq!(page.events.pop(), Some(expected));
    }
}
