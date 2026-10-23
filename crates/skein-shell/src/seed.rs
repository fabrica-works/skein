//! The seed (shell.md, 5; programming-model.md, section 9).

use crate::system;
use skein_lib::Secret;

/// A random seed for the layers' `Rng`s, from the system. Read once, at
/// startup: everything random after it is drawn from the layers' state.
///
/// Fails with the error number when the system cannot fill the seed.
pub fn seed() -> Result<u64, i32> {
    let mut bytes = [0_u8; 8];
    system::random(&mut bytes)?;
    Ok(u64::from_ne_bytes(bytes))
}

/// A separate 32-byte cryptographic seed drawn once at startup (shell.md, 5).
/// Fails with the kernel's error number when the system cannot fill it.
pub fn crypto_seed() -> Result<Secret, i32> {
    let mut bytes = [0_u8; 32];
    system::random(&mut bytes)?;
    Ok(Secret::new(Box::from(bytes)))
}
