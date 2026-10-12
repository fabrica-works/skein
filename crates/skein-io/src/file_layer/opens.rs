//! Existing file opens and cuts (io.md, 5).
use super::{FileIo, Pending, terminal_failure};
use crate::file::Event;
use crate::kernel::{Error, Fd, Op, OpenHow, Submit};
use alloc::boxed::Box;
use skein_lib::{Queue, Token};

pub(super) fn open_read(
    io: &mut FileIo,
    owner: Token,
    root: Fd,
    name: Box<[u8]>,
    how: OpenHow,
    events: &mut Queue<Event>,
    subs: &mut Queue<Submit>,
) {
    if name.contains(&0) {
        events.push(Event::Failed { owner, error: Error::InvalidArgument, committed: false, residue: None });
        return;
    }
    if io.files.len() == io.files.capacity() {
        events.push(Event::Failed { owner, error: Error::TooManyOpenFiles, committed: false, residue: None });
        return;
    }
    io.pending = Some(Pending::OpenRead { owner });
    io.issue(Op::Open { root, path: name, how }, subs);
}

pub(super) fn cut(
    io: &mut FileIo,
    owner: Token,
    file: Token,
    length: u64,
    events: &mut Queue<Event>,
    subs: &mut Queue<Submit>,
) {
    let Some(fd) = io.file(file) else {
        terminal_failure(events, owner, Error::NotFound);
        return;
    };
    let op = Op::Truncate { fd, length };
    if !op.is_valid() {
        terminal_failure(events, owner, Error::InvalidArgument);
        return;
    }
    io.pending = Some(Pending::Cut { owner });
    io.issue(op, subs);
}
