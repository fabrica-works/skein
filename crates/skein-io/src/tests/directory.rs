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
