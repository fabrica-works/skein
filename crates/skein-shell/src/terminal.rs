//! Startup terminal modes and window sizes (shell.md, section 6.3).
use skein_io::kernel::{Error, Fd, Size};

/// Whether interrupt keys keep their terminal signals once raw.
#[derive(Clone, Copy, Debug)]
pub enum Keys {
    /// Keep the terminal making interrupt signals.
    Signal,
    /// Pass interrupt keys through as input bytes.
    Pass,
}
/// Which signals startup reads from its one signalfd.
#[derive(Clone, Copy, Debug)]
pub enum Watch {
    /// Read SIGINT and SIGTERM.
    Termination,
    /// Read SIGINT, SIGTERM and SIGWINCH.
    TerminationAndWindow,
}
/// What startup found on its input descriptor.
pub enum Input {
    /// Raw input was set, with its original modes and startup size.
    Terminal { kept: Kept, size: Size },
    /// Input was left as it was.
    NotATerminal,
}
/// The saved modes and a duplicate input descriptor, kept until drive returns.
pub struct Kept {
    pub(crate) fd: Fd,
    pub(crate) modes: libc::termios,
}
/// The modes visible from a terminal's other end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Modes {
    pub echo: bool,
    pub lines: bool,
    pub signals: bool,
}
/// Sets input to key-at-a-time reading, retaining the modes for restoration.
pub fn raw_input(input: Fd, keys: Keys) -> Result<Input, Error> {
    crate::system::terminal::raw_input(input, keys)
}
/// Blocks the selected signals and opens the signalfd io adopts.
pub fn open_signals(watch: Watch) -> Result<Fd, i32> {
    crate::system::process::open_signals(watch)
}
impl Kept {
    /// Restores startup's modes after drive returns, before the last stderr line.
    pub fn restore(self) -> Result<(), Error> {
        crate::system::terminal::restore(self.fd, &self.modes)
    }
}
/// Sets a pseudo-terminal size from its master, for a scripted person.
pub fn resize_terminal(master: Fd, size: Size) -> Result<(), Error> {
    crate::system::terminal::resize(master, size)
}
/// Reads modes from the other end of a terminal, for a scripted person.
pub fn terminal_modes(fd: Fd) -> Result<Modes, Error> {
    crate::system::terminal::modes(fd)
}
