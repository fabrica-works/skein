//! The shared listing face's buffer boundary, startup identity, and the
//! blocking flags on a regular file after Open.

use std::fs;

use skein_io::kernel::{Complete, Done, Entry, Op, OpenHow, Submit};
use skein_lib::{Queue, Token};
use skein_scratch::Scratch;

use crate::Kernel;
use crate::system::{open_root, status_flags};
fn close(fd: i32) {
    crate::system::close(skein_io::kernel::Fd::new(fd)).expect("close test descriptor");
}
use crate::{Config, Wait};

#[test]
fn startup_process_id_is_this_process() {
    assert_eq!(crate::process_id(), std::process::id());
}

#[test]
fn a_name_that_fits_in_no_names_is_too_long_and_stays_next() {
    let scratch = Scratch::new("shell");
    fs::write(scratch.path().join("a-name-of-twenty-bytes"), b"").expect("a file laid");
    let dir = open_root(scratch.path()).expect("a scratch directory opens");
    let mut listing = crate::system::Listings::new();
    let mut entries = [Entry::BLANK; 2];
    let mut small = [0_u8; 8];
    assert_eq!(listing.list(dir, &mut entries, &mut small), Err(libc::ENAMETOOLONG));
    let mut names = [0_u8; 255];
    assert_eq!(listing.list(dir, &mut entries, &mut names), Ok(1), "the name is still next");
    let first = entries.first().expect("an entry").name(&names);
    assert_eq!(first, Some(&b"a-name-of-twenty-bytes"[..]));
    assert_eq!(listing.list(dir, &mut entries, &mut names), Ok(0), "then the end");
    listing.forget(dir);
    close(dir.raw());
}

/// Every `Open` goes down without blocking, so that a FIFO opens to be
/// refused; a file it opened is blocking again, so that io_uring sends a
/// read it cannot do at once to its worker (kernel.md, 6.1).
#[test]
fn a_file_opened_blocks_again() {
    let scratch = Scratch::new("shell");
    fs::write(scratch.path().join("f"), b"f").expect("a file laid");
    let dir = open_root(scratch.path()).expect("a scratch directory opens");
    let mut kernel = Kernel::open(Config { operations: 4 }).expect("the kernel is usable here");
    let mut submissions = Queue::with_capacity(1);
    let path = Box::from(&b"f"[..]);
    submissions.push(Submit { op: Token::new(1), kind: Op::Open { root: dir, path, how: OpenHow::Read } });
    kernel.submit(&mut submissions, Wait::Forever);
    let mut completions = Queue::with_capacity(1);
    kernel.reap(&mut completions);
    let Some(Complete { result: Ok(Done::Fd(file)), .. }) = completions.pop() else { panic!("the file opened") };
    let flags = status_flags(file.raw()).expect("an open descriptor's flags");
    assert_eq!(flags & libc::O_NONBLOCK, 0, "the file blocks");
    close(file.raw());
    close(dir.raw());
}

#[test]
fn make_root_makes_and_opens_a_nested_absent_path() {
    let scratch = Scratch::new("make-root");
    let path = scratch.path().join("state/private");
    let fd = crate::make_root(&path, 0o700).expect("nested root made and opened");
    assert!(path.is_dir(), "the last directory exists");
    assert_eq!(open_root(&path).map(|opened| close(opened.raw())), Ok(()), "opens as a root");
    close(fd.raw());
}

#[test]
fn make_root_refuses_a_file_in_its_path() {
    let scratch = Scratch::new("blocked-root");
    let file = scratch.path().join("file");
    fs::write(&file, b"contents").expect("blocking file");
    for (label, path) in [("final file", file.clone()), ("parent file", file.join("private"))] {
        assert_eq!(crate::make_root(&path, 0o700), Err(libc::ENOTDIR), "{label}");
    }
    assert_eq!(fs::read(file).expect("blocking file remains"), b"contents", "file contents retained");
}

#[test]
fn make_root_preserves_an_existing_directorys_mode() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("existing-root");
    fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o711)).expect("existing mode");
    let fd = crate::make_root(scratch.path(), 0o700).expect("existing root opened");
    close(fd.raw());
    assert_eq!(
        fs::metadata(scratch.path()).expect("existing directory").permissions().mode() & 0o777,
        0o711,
        "existing mode retained"
    );
}
