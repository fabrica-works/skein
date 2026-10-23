//! The opens of the kernel boundary (kernel.md, 6.1).
use skein_io::kernel::OpenHow;

/// The new file's mode a `Create` that asks for none is made with, and a
/// `MakeDirectory`'s, both less the umask (`skein_io::kernel`, backend
/// defaults).
const FILE_MODE: u64 = 0o666;

/// The Linux openat2 fields shared by both backends.
pub(crate) struct Fields {
    pub flags: u64,
    pub mode: u64,
    pub resolve: u64,
}

/// What `openat2` is told for each way io opens: close-on-exec always,
/// without blocking or taking a controlling terminal, and resolved beneath
/// the root, without magic links (kernel.md, 6.1).
pub(crate) fn fields(how: OpenHow) -> Fields {
    let no_follow = matches!(
        how,
        OpenHow::PathNoFollow
            | OpenHow::ReadNoFollow
            | OpenHow::WriteNoFollow
            | OpenHow::DirectoryNoFollow
            | OpenHow::CreateNoFollow { .. }
    );
    let (flags, mode) = match how {
        OpenHow::PathNoFollow => (libc::O_PATH | libc::O_NOFOLLOW, 0),
        OpenHow::Write | OpenHow::WriteNoFollow => (libc::O_RDWR, 0),
        OpenHow::Read | OpenHow::ReadNoFollow => (libc::O_RDONLY, 0),
        OpenHow::Directory | OpenHow::DirectoryNoFollow => (libc::O_RDONLY | libc::O_DIRECTORY, 0),
        OpenHow::Create { mode } | OpenHow::CreateNoFollow { mode } => {
            let mode = match mode {
                Some(mode) => u64::from(mode),
                None => FILE_MODE,
            };
            (libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL, mode)
        }
    };
    let flags = if matches!(how, OpenHow::PathNoFollow) {
        flags | libc::O_CLOEXEC
    } else {
        flags | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_NOCTTY
    };
    let flags = u64::try_from(flags).expect("open's flags are positive");
    let resolve = if no_follow {
        libc::RESOLVE_BENEATH | libc::RESOLVE_NO_SYMLINKS
    } else {
        libc::RESOLVE_BENEATH | libc::RESOLVE_NO_MAGICLINKS
    };
    Fields { flags, mode, resolve }
}
