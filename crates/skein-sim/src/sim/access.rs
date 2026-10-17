//! Descriptor access rules for files (kernel.md, 6.1).
use super::{Pid, Sim, files};
use crate::trace::Summary;
use alloc::format;
use skein_io::kernel::OpenHow;
impl Sim {
    /// Fails the world on a broken invariant of an operation on files whose
    /// descriptors are open, or of any other on a file's: each a file's, and
    /// opened as the operation needs; only a `Close` beside an operation on
    /// sockets, and nothing in flight beside it.
    pub(super) fn check_file(&self, pid: Pid, kind: Summary) {
        let process = self.process(pid);
        for fd in kind.fds().into_iter().flatten() {
            if let Some(file) = process.files.get(&fd)
                && file.access == files::Access::Record(OpenHow::PathNoFollow)
                && !matches!(kind, Summary::Stat { .. } | Summary::Close { .. })
            {
                self.fail(pid, "only Stat and Close may use a path-only descriptor");
            }
        }
        if let Summary::Close { fd } = kind {
            for flight in process.flights.values() {
                if flight.kind.fds().contains(&Some(fd)) {
                    self.fail(pid, &format!("a Close while {:?} is in flight on {fd:?}", flight.kind));
                }
            }
            return;
        }
        if !kind.is_file() {
            self.fail(pid, &format!("an operation on sockets on a file's descriptor: {kind:?}"));
        }
        for fd in kind.fds().into_iter().flatten() {
            if process.fds.contains_key(&fd) {
                self.fail(pid, &format!("an operation on files on {fd:?}, a socket's descriptor: {kind:?}"));
            }
        }
        let Some(fd) = kind.fd() else {
            self.bug("an operation on files names a descriptor");
        };
        let how = process.files.get(&fd).expect("checked open, and not a socket's").access;
        let broken = match kind {
            Summary::Read { .. } if !can_read(how) => Some("a Read on a descriptor not opened to read"),
            Summary::Write { .. } if !can_write(how) => Some("a Write on a descriptor not opened to create"),
            Summary::Truncate { .. }
                if !matches!(how, files::Access::Record(OpenHow::Write | OpenHow::WriteNoFollow)) =>
            {
                Some("a Truncate on a descriptor not opened to write")
            }
            Summary::Append { .. } if !can_append(how) => Some("an Append on a descriptor not opened to write"),
            Summary::Append { fd, .. }
                if process
                    .flights
                    .values()
                    .any(|flight| matches!(flight.kind, Summary::Append { fd: other, .. } if other == fd)) =>
            {
                Some("a second Append in flight")
            }
            Summary::List { .. }
                if matches!(
                    how,
                    files::Access::Append
                        | files::Access::Record(OpenHow::Create { .. } | OpenHow::CreateNoFollow { .. })
                ) =>
            {
                Some("a List on a descriptor opened to create")
            }
            Summary::Open { .. }
            | Summary::Read { .. }
            | Summary::Write { .. }
            | Summary::Append { .. }
            | Summary::Truncate { .. }
            | Summary::Sync { .. }
            | Summary::Lock { .. }
            | Summary::Stat { .. }
            | Summary::Rename { .. }
            | Summary::Remove { .. }
            | Summary::MakeDirectory { .. }
            | Summary::List { .. } => None,
            Summary::Socket { .. }
            | Summary::Bind { .. }
            | Summary::Listen { .. }
            | Summary::Accept { .. }
            | Summary::Connect { .. }
            | Summary::Recv { .. }
            | Summary::Send { .. }
            | Summary::Shutdown { .. }
            | Summary::Close { .. }
            | Summary::Cancel { .. } => self.bug("only an operation on files is checked as one"),
            Summary::Spawn { .. }
            | Summary::Wait { .. }
            | Summary::Signal { .. }
            | Summary::Window { .. }
            | Summary::Usage
            | Summary::ReadSignal { .. }
            | Summary::PipeRead { .. }
            | Summary::PipeWrite { .. } => self.bug("a process operation passed file checks"),
        };
        if let Some(broken) = broken {
            self.fail(pid, &format!("{broken}, opened {how:?}: {kind:?}"));
        }
    }
}

fn can_read(how: files::Access) -> bool {
    matches!(
        how,
        files::Access::Record(OpenHow::Read | OpenHow::ReadNoFollow | OpenHow::Write | OpenHow::WriteNoFollow)
    )
}

fn can_write(how: files::Access) -> bool {
    matches!(
        how,
        files::Access::Record(
            OpenHow::Create { .. } | OpenHow::CreateNoFollow { .. } | OpenHow::Write | OpenHow::WriteNoFollow
        )
    )
}

fn can_append(how: files::Access) -> bool {
    how == files::Access::Append || can_write(how)
}
