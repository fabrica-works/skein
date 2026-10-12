//! Opening an existing file to write, and setting its length (kernel.md, 6.1).
use crate::files::rule;
use crate::run::Run;
use crate::{Backend, Check, Item};
use alloc::vec::Vec;
use skein_io::kernel::{Error, Fd, OpenHow};

/// What `Write` opens found and what `Truncate`s left (kernel.md, 6.1).
#[derive(Debug)]
pub struct Cutting {
    pub missing: Result<Fd, Error>,
    pub after_missing: Result<Vec<u8>, Error>,
    pub shortened: Vec<u8>,
    pub lengthened: Vec<u8>,
    pub refused: [Error; 3],
}

/// A `Write` open of a missing file makes none; one of a file opens it;
/// a `Truncate` shortens it, then lengthens it with zeros.
#[must_use]
pub fn cutting<B: Backend>(backend: &mut B) -> Cutting {
    let mut run = Run::new(backend);
    let process = run.process();
    let tree =
        [Item::file(b"f", b"0123456789"), Item::directory(b"d"), Item::fifo(b"pipe"), Item::link(b"inside", b"f")];
    let root = run.root(process, &tree);
    let missing = run.open(process, root, b"missing", OpenHow::Write);
    let after_missing = run.contents(process, root, b"missing");
    let fd = run.open(process, root, b"f", OpenHow::Write).expect("an existing file opens to write");
    run.truncate(process, fd, 4).expect("cut to four bytes");
    let shortened = run.contents(process, root, b"f").expect("read back");
    run.truncate(process, fd, 6).expect("cut past the end");
    let lengthened = run.contents(process, root, b"f").expect("read back");
    run.close(process, fd);
    let refused = [
        run.open(process, root, b"d", OpenHow::Write).expect_err("a directory does not open to write"),
        run.open(process, root, b"pipe", OpenHow::Write).expect_err("a FIFO opens as no file"),
        run.open(process, root, b"inside", OpenHow::WriteNoFollow).expect_err("no link is followed"),
    ];
    run.close(process, root);
    run.finish();
    Cutting { missing, after_missing, shortened, lengthened, refused }
}

impl Check for Cutting {
    fn check(&self) {
        rule(self.missing == Err(Error::NotFound), "Write never creates: a missing file is NotFound", self);
        rule(self.after_missing == Err(Error::NotFound), "and nothing was made", self);
        rule(self.shortened == b"0123", "Truncate drops what lay past the length", self);
        rule(self.lengthened == b"0123\0\0", "Truncate past the end reads as zeros", self);
        rule(
            self.refused == [Error::IsADirectory, Error::NotAFile, Error::TooManyLinks],
            "Write refuses a directory and a FIFO, and its no-follow form a link",
            self,
        );
    }
}
