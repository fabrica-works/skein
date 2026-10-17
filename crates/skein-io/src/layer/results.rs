//! Kernel result interpretation for io (kernel.md, sections 5 and 6.3).

use crate::kernel::{self, Done};

pub(super) fn window(result: Result<Done, kernel::Error>) -> crate::Window {
    match result {
        Ok(Done::Window(size)) => crate::Window::Size(size),
        Err(kernel::Error::Other(_)) => crate::Window::Gone,
        Ok(
            Done::Nothing
            | Done::Count(_)
            | Done::Fd(_)
            | Done::Accepted { .. }
            | Done::Bound(_)
            | Done::Stat(_)
            | Done::Spawned { .. }
            | Done::Exit(_)
            | Done::Signalled(_)
            | Done::Usage(_),
        ) => unreachable!("Window answers its size"),
        Err(
            kernel::Error::Refused
            | kernel::Error::Reset
            | kernel::Error::BrokenPipe
            | kernel::Error::NotConnected
            | kernel::Error::AddressInUse
            | kernel::Error::AddressNotAvailable
            | kernel::Error::Unreachable
            | kernel::Error::TimedOut
            | kernel::Error::TooManyOpenFiles
            | kernel::Error::NoBufferSpace
            | kernel::Error::InvalidArgument
            | kernel::Error::Cancelled
            | kernel::Error::TooLate
            | kernel::Error::NotFound
            | kernel::Error::Exists
            | kernel::Error::NotADirectory
            | kernel::Error::IsADirectory
            | kernel::Error::NotEmpty
            | kernel::Error::Permission
            | kernel::Error::NoSpace
            | kernel::Error::ReadOnly
            | kernel::Error::TooManyLinks
            | kernel::Error::NameTooLong
            | kernel::Error::Escape
            | kernel::Error::Busy
            | kernel::Error::NotAFile,
        ) => unreachable!("Window fails only Other (kernel.md, section 6.3)"),
    }
}

/// Whether a cancel's answer says the backend did not submit it, so that its
/// target runs on. Stopped (`Ok`) or too late, its target completes by itself.
pub(crate) fn unsubmitted(result: Result<Done, kernel::Error>) -> bool {
    match result {
        Ok(Done::Nothing) | Err(kernel::Error::TooLate) => false,
        Err(kernel::Error::InvalidArgument | kernel::Error::Other(_)) => true,
        Ok(
            Done::Count(_)
            | Done::Fd(_)
            | Done::Accepted { .. }
            | Done::Bound(_)
            | Done::Stat(_)
            | Done::Spawned { .. }
            | Done::Exit(_)
            | Done::Usage(_)
            | Done::Signalled(_)
            | Done::Window(_),
        ) => {
            unreachable!("a cancel answers with nothing")
        }
        Err(
            kernel::Error::Refused
            | kernel::Error::Reset
            | kernel::Error::BrokenPipe
            | kernel::Error::NotConnected
            | kernel::Error::AddressInUse
            | kernel::Error::AddressNotAvailable
            | kernel::Error::Unreachable
            | kernel::Error::TimedOut
            | kernel::Error::TooManyOpenFiles
            | kernel::Error::NoBufferSpace
            | kernel::Error::Cancelled
            | kernel::Error::NotFound
            | kernel::Error::Exists
            | kernel::Error::NotADirectory
            | kernel::Error::IsADirectory
            | kernel::Error::NotEmpty
            | kernel::Error::Permission
            | kernel::Error::NoSpace
            | kernel::Error::ReadOnly
            | kernel::Error::TooManyLinks
            | kernel::Error::NameTooLong
            | kernel::Error::Escape
            | kernel::Error::Busy
            | kernel::Error::NotAFile,
        ) => unreachable!("a cancel fails only too late or unsubmitted (kernel.md, 5)"),
    }
}
