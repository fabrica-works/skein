//! Signed kernel file offsets (kernel.md, 6.1).
/// Whether `len` bytes from offset `at` end within the largest offset a file
/// has, `i64::MAX`: the kernel's offsets are signed, and the ring reads an
/// offset of `u64::MAX` as the descriptor's position.
pub(super) fn within_offsets(at: u64, len: usize) -> bool {
    let (Ok(len), Ok(largest)) = (u64::try_from(len), u64::try_from(i64::MAX)) else {
        return false;
    };
    match at.checked_add(len) {
        Some(end) => end <= largest,
        None => false,
    }
}
