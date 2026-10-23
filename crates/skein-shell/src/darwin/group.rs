//! Darwin's tree keeper calls (shell.md, 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! Kill takes only a checked, negative process group ID and a signal number;
//! it borrows no memory and retains no pointer.

#![expect(unsafe_code, reason = "kill takes checked integers only (shell.md, 7.5)")]

use skein_io::kernel::{Error, Fd, Signal};

use super::{last_errno, process::map_error};

pub(crate) fn child_id(handle: Fd) -> Result<u32, Error> {
    super::process::child_id(handle)
}

fn negative_group(group: u32) -> Result<libc::pid_t, Error> {
    let positive = libc::pid_t::try_from(group).map_err(|_| Error::InvalidArgument)?;
    // Negating 0 or 1 would select the caller's group or every permitted process.
    if positive <= 1 {
        return Err(Error::InvalidArgument);
    }
    positive.checked_neg().ok_or(Error::InvalidArgument)
}

pub(crate) fn signal_group(group: u32, signal: Signal) -> Result<(), Error> {
    let group = negative_group(group)?;
    let number = super::process::signal_number(signal);
    // SAFETY: group is a checked negative ID; kill takes only integers.
    let sent = unsafe { libc::kill(group, number) };
    if sent == 0 { Ok(()) } else { Err(map_error(last_errno())) }
}

pub(crate) fn group_live(group: u32) -> Result<bool, Error> {
    let group = negative_group(group)?;
    // SAFETY: group is a checked negative ID; signal zero probes membership.
    if unsafe { libc::kill(group, 0) } == 0 {
        return Ok(true);
    }
    match last_errno() {
        libc::ESRCH => Ok(false),
        libc::EPERM => Ok(true),
        other => Err(map_error(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::{group_live, negative_group, signal_group};
    use skein_io::kernel::{Error, Signal};

    #[test]
    fn only_specific_positive_groups_can_be_addressed() {
        for group in [0, 1, u32::MAX] {
            assert_eq!(negative_group(group), Err(Error::InvalidArgument));
            assert_eq!(group_live(group), Err(Error::InvalidArgument));
            assert_eq!(signal_group(group, Signal::Kill), Err(Error::InvalidArgument));
        }
        assert_eq!(negative_group(2), Ok(-2));
        assert_eq!(negative_group(i32::MAX.unsigned_abs()), Ok(-i32::MAX));
    }
}
