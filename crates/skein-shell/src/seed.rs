//! The seed (shell.md, 5; programming-model.md, section 9).

use crate::system;
use skein_lib::Secret;

/// A random seed for the layers' `Rng`s, from `getrandom`. Read once, at
/// startup: everything random after it is drawn from the layers' state.
///
/// Fails with the error number when the kernel refuses `getrandom` (a
/// seccomp profile), or a signal interrupts it while the entropy pool is
/// still initialising (`EINTR`).
pub fn seed() -> Result<u64, i32> {
    let mut bytes = [0_u8; 8];
    system::random(&mut bytes)?;
    Ok(u64::from_ne_bytes(bytes))
}

/// A separate 32-byte cryptographic seed drawn once at startup (shell.md, 5).
/// Fails with the kernel's error number when `getrandom` cannot fill it.
pub fn crypto_seed() -> Result<Secret, i32> {
    let mut bytes = [0_u8; 32];
    system::random(&mut bytes)?;
    Ok(Secret::new(Box::from(bytes)))
}
