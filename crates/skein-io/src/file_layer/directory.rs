//! Pages use the owner's open directory as their cursor (io.md, 5).
use super::{Complete, Done, Entry, Error, Event, Fd, FileIo, Op, Pending, Queue, Submit, Token, Vec, vec};
use alloc::boxed::Box;

impl FileIo {
    fn list_at_most(&mut self, fd: Fd, most: u32, subs: &mut Queue<Submit>) {
        self.issue(
            Op::List {
                fd,
                entries: vec![crate::kernel::Entry::BLANK; usize::try_from(most.min(32)).expect("u32 fits usize")]
                    .into_boxed_slice(),
                names: vec![0_u8; 8192].into_boxed_slice(),
            },
            subs,
        );
    }
}

pub(super) fn page(
    io: &mut FileIo,
    owner: Token,
    directory: Token,
    max: u32,
    events: &mut Queue<Event>,
    subs: &mut Queue<Submit>,
) {
    let Some(fd) = io.file(directory) else {
        super::terminal_failure(events, owner, Error::NotFound);
        return;
    };
    if max == 0 || max > io.max_entries {
        super::terminal_failure(events, owner, Error::InvalidArgument);
        return;
    }
    io.pending = Some(Pending::Page {
        owner,
        fd,
        max,
        entries: Vec::with_capacity(usize::try_from(max).expect("u32 fits usize")),
    });
    io.list_at_most(fd, max, subs);
}

pub(super) fn answered(
    io: &mut FileIo,
    page: Pending,
    complete: Complete,
    stopped: Option<Error>,
    events: &mut Queue<Event>,
    subs: &mut Queue<Submit>,
) {
    let Pending::Page { owner, fd, max, mut entries } = page else { unreachable!("a page completion") };
    let count = match complete.result {
        Ok(Done::Count(count)) => count,
        Err(error) => {
            super::terminal_failure(events, owner, stopped.unwrap_or(error));
            return;
        }
        Ok(_) => unreachable!("a list answers a count"),
    };
    let Op::List { entries: records, names, .. } = complete.kind else { unreachable!("a page lists") };
    for record in records.get(..usize::try_from(count).expect("u32 fits usize")).expect("count within list") {
        let name = record.name(&names).expect("a listed name is within its buffer");
        entries.push(Entry { name: Box::from(name), kind: record.kind });
    }
    let held = u32::try_from(entries.len()).expect("a page is bounded by max");
    if count == 0 || held == max {
        events.push(Event::Listed { owner, entries: entries.into_boxed_slice() });
    } else if let Some(reason) = stopped {
        super::terminal_failure(events, owner, reason);
    } else {
        io.pending = Some(Pending::Page { owner, fd, max, entries });
        io.list_at_most(fd, max.checked_sub(held).expect("the list never takes past max"), subs);
    }
}
