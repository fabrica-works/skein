//! A delivery's UUID text (forge-github.md, 9.2).

/// Reads exactly 8-4-4-4-12 hexadecimal digits, in either case.
#[must_use]
pub fn read(text: &[u8]) -> Option<[u8; 16]> {
    if text.len() != 36 {
        return None;
    }
    let mut guid = [0_u8; 16];
    let mut digits = 0_usize;
    for (at, &byte) in text.iter().enumerate() {
        if [8, 13, 18, 23].contains(&at) {
            if byte != b'-' {
                return None;
            }
            continue;
        }
        let value = match byte {
            b'0'..=b'9' => byte.checked_sub(b'0')?,
            b'a'..=b'f' => byte.checked_sub(b'a')?.checked_add(10)?,
            b'A'..=b'F' => byte.checked_sub(b'A')?.checked_add(10)?,
            _ => return None,
        };
        let target = guid.get_mut(digits.checked_div(2)?)?;
        *target = target.checked_mul(16)?.checked_add(value)?;
        digits = digits.checked_add(1)?;
    }
    Some(guid)
}

/// Writes lowercase, hyphenated text.
#[must_use]
pub fn write(guid: &[u8; 16]) -> [u8; 36] {
    let mut text = [b'-'; 36];
    let mut digits = 0_usize;
    for (at, target) in text.iter_mut().enumerate() {
        if [8, 13, 18, 23].contains(&at) {
            continue;
        }
        let byte = guid.get(digits.checked_div(2).expect("divisor is two")).expect("32 UUID digits");
        let value = if digits.is_multiple_of(2) { byte >> 4_u32 } else { byte & 15 };
        *target = *b"0123456789abcdef".get(usize::from(value)).expect("a hexadecimal digit");
        digits = digits.checked_add(1).expect("32 UUID digits");
    }
    text
}
