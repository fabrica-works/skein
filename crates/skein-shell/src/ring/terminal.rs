//! Terminal syscalls shared by the Linux backends (shell.md, sections 6.3 and 7.5).
use crate::terminal::{Input, Kept, Keys, Modes};
use skein_io::kernel::{Error, Fd, Size};
use std::{mem::MaybeUninit, ptr};

fn read(fd: Fd) -> Result<libc::termios, Error> {
    let mut modes = MaybeUninit::uninit();
    // SAFETY: tcgetattr writes a complete termios into this live allocation.
    if unsafe { libc::tcgetattr(fd.raw(), modes.as_mut_ptr()) } != 0 {
        return Err(Error::Other(super::last_errno()));
    }
    // SAFETY: successful tcgetattr initialized every termios field.
    Ok(unsafe { modes.assume_init() })
}
fn set(fd: Fd, modes: &libc::termios) -> Result<(), Error> {
    // SAFETY: modes is an initialized termios, borrowed for this synchronous call.
    if unsafe { libc::tcsetattr(fd.raw(), libc::TCSANOW, ptr::from_ref(modes)) } == 0 {
        Ok(())
    } else {
        Err(Error::Other(super::last_errno()))
    }
}
pub(crate) fn size(fd: Fd) -> Result<Size, Error> {
    let mut size = MaybeUninit::<libc::winsize>::uninit();
    // SAFETY: TIOCGWINSZ writes one winsize into this live allocation.
    if unsafe { libc::ioctl(fd.raw(), libc::TIOCGWINSZ, size.as_mut_ptr()) } != 0 {
        return Err(Error::Other(super::last_errno()));
    }
    // SAFETY: successful ioctl initialized the winsize.
    let size = unsafe { size.assume_init() };
    Ok(Size { rows: size.ws_row, columns: size.ws_col })
}
pub(crate) fn raw_input(fd: Fd, keys: Keys) -> Result<Input, Error> {
    let modes = match read(fd) {
        Ok(modes) => modes,
        Err(Error::Other(libc::ENOTTY)) => return Ok(Input::NotATerminal),
        Err(error) => return Err(error),
    };
    let size = size(fd)?;
    // SAFETY: fcntl duplicates the live input descriptor with close-on-exec.
    let saved = unsafe { libc::fcntl(fd.raw(), libc::F_DUPFD_CLOEXEC, 3) };
    if saved < 0 {
        return Err(Error::Other(super::last_errno()));
    }
    let kept = Kept { fd: Fd::new(saved), modes };
    let mut raw = kept.modes;
    raw.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON);
    match keys {
        Keys::Signal => raw.c_lflag |= libc::ISIG,
        Keys::Pass => raw.c_lflag &= !libc::ISIG,
    }
    *raw.c_cc.get_mut(libc::VMIN).expect("termios has VMIN") = 1;
    *raw.c_cc.get_mut(libc::VTIME).expect("termios has VTIME") = 0;
    if let Err(error) = set(fd, &raw) {
        // SAFETY: saved is the descriptor just opened above, uniquely owned here.
        unsafe { libc::close(saved) };
        return Err(error);
    }
    Ok(Input::Terminal { kept, size })
}
pub(crate) fn restore(fd: Fd, modes: &libc::termios) -> Result<(), Error> {
    let result = set(fd, modes);
    // SAFETY: this is the duplicated descriptor consumed by Kept::restore.
    unsafe { libc::close(fd.raw()) };
    result
}
pub(crate) fn modes(fd: Fd) -> Result<Modes, Error> {
    let modes = read(fd)?;
    Ok(Modes {
        echo: modes.c_lflag & libc::ECHO != 0,
        lines: modes.c_lflag & libc::ICANON != 0,
        signals: modes.c_lflag & libc::ISIG != 0,
    })
}
pub(crate) fn resize(fd: Fd, size: Size) -> Result<(), Error> {
    let size = libc::winsize { ws_row: size.rows, ws_col: size.columns, ws_xpixel: 0, ws_ypixel: 0 };
    // SAFETY: TIOCSWINSZ reads the initialized winsize during this synchronous call.
    if unsafe { libc::ioctl(fd.raw(), libc::TIOCSWINSZ, ptr::from_ref(&size)) } == 0 {
        Ok(())
    } else {
        Err(Error::Other(super::last_errno()))
    }
}
