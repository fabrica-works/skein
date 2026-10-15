//! Native date/time parts under the kit's en-US locale (ui-web.md, 14.2).
pub(crate) fn date(value: &[u8]) -> Option<Box<[u8]>> {
    if value.is_empty() {
        return Some(Box::from([]));
    }
    if value.len() != 10 {
        return None;
    }
    for (index, byte) in value.iter().enumerate() {
        if index == 4 || index == 7 {
            if *byte != b'-' {
                return None;
            }
        } else if !byte.is_ascii_digit() {
            return None;
        }
    }
    let mut keys = Vec::with_capacity(8);
    keys.extend_from_slice(&value[5..7]);
    keys.extend_from_slice(&value[8..]);
    keys.extend_from_slice(&value[..4]);
    Some(keys.into_boxed_slice())
}

pub(crate) fn time(value: &[u8]) -> Option<Box<[u8]>> {
    if value.is_empty() {
        return Some(Box::from([]));
    }
    if value.len() != 5 || value[2] != b':' {
        return None;
    }
    let hour = skein_ui::number(&value[..2])?;
    let minute = skein_ui::number(&value[3..])?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let half = if hour < 12 { b'A' } else { b'P' };
    let hour = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    let tens = u8::try_from(hour / 10).expect("two-digit hour");
    let ones = u8::try_from(hour % 10).expect("two-digit hour");
    Some(Box::from([b'0' + tens, b'0' + ones, value[3], value[4], half]))
}
