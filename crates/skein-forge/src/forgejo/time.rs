//! RFC 3339 at second resolution, with checked integer arithmetic.
use skein_lib::Wall;
pub const RFC3339_BYTES: u32 = 20;
fn number(text: &[u8], start: usize, end: usize) -> Option<u64> {
    super::id::unsigned(text.get(start..end)?)
}
fn leap(year: u64) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}
fn month_days(year: u64, month: u64) -> Option<u64> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if leap(year) { 29 } else { 28 }),
        _ => None,
    }
}
fn before(year: u64) -> Option<i64> {
    let previous = year.checked_sub(1)?;
    i64::try_from(
        previous
            .checked_mul(365)?
            .checked_add(previous / 4)?
            .checked_sub(previous / 100)?
            .checked_add(previous / 400)?,
    )
    .ok()
}
#[must_use]
pub fn read(text: &[u8]) -> Option<Wall> {
    for (at, byte) in [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')] {
        if text.get(at) != Some(&byte) {
            return None;
        }
    }
    let year = number(text, 0, 4)?;
    let month = number(text, 5, 7)?;
    let day = number(text, 8, 10)?;
    let hour = number(text, 11, 13)?;
    let minute = number(text, 14, 16)?;
    let second = number(text, 17, 19)?;
    if day == 0 || day > month_days(year, month)? || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut at = 19usize;
    if text.get(at) == Some(&b'.') {
        at = at.checked_add(1)?;
        let start = at;
        for byte in text.get(at..)? {
            if !byte.is_ascii_digit() {
                break;
            }
            at = at.checked_add(1)?;
        }
        if at == start {
            return None;
        }
    }
    let offset = match text.get(at..)? {
        b"Z" => 0i64,
        tail if tail.len() == 6
            && (tail.first() == Some(&b'+') || tail.first() == Some(&b'-'))
            && tail.get(3) == Some(&b':') =>
        {
            let h = number(tail, 1, 3)?;
            let m = number(tail, 4, 6)?;
            if h > 23 || m > 59 {
                return None;
            }
            let seconds = i64::try_from(h.checked_mul(3600)?.checked_add(m.checked_mul(60)?)?).ok()?;
            if tail.first() == Some(&b'-') { seconds.checked_neg()? } else { seconds }
        }
        _ => return None,
    };
    let mut days = before(year)?.checked_sub(before(1970)?)?;
    for m in 1..month {
        days = days.checked_add(i64::try_from(month_days(year, m)?).ok()?)?;
    }
    days = days.checked_add(i64::try_from(day.checked_sub(1)?).ok()?)?;
    let daytime = hour.checked_mul(3600)?.checked_add(minute.checked_mul(60)?)?.checked_add(second)?;
    let seconds = days.checked_mul(86400)?.checked_add(i64::try_from(daytime).ok()?)?.checked_sub(offset)?;
    Some(Wall::from_nanos(u64::try_from(seconds).ok()?.checked_mul(1_000_000_000)?))
}
#[must_use]
pub fn write(wall: Wall) -> [u8; 20] {
    let seconds = wall.as_secs();
    let mut days = seconds / 86400;
    let mut year = 1970;
    for y in 1970..=2554 {
        let count = if leap(y) { 366 } else { 365 };
        if days < count {
            year = y;
            break;
        }
        days = days.saturating_sub(count);
    }
    let mut month = 1;
    for m in 1..=12 {
        let count = month_days(year, m).expect("a calendar month");
        if days < count {
            month = m;
            break;
        }
        days = days.saturating_sub(count);
    }
    let mut output = *b"0000-00-00T00:00:00Z";
    for (start, width, mut value) in [
        (0usize, 4usize, year),
        (5, 2, month),
        (8, 2, days.saturating_add(1)),
        (11, 2, seconds / 3600 % 24),
        (14, 2, seconds / 60 % 60),
        (17, 2, seconds % 60),
    ] {
        for pos in (start..start.saturating_add(width)).rev() {
            *output.get_mut(pos).expect("fixed date layout") =
                b'0'.saturating_add(u8::try_from(value % 10).expect("digit"));
            value /= 10;
        }
    }
    output
}
/// Reads IMF-fixdate, the only HTTP date syntax used by these documents.
#[must_use]
pub fn http_date(text: &[u8]) -> Option<Wall> {
    if text.len() != 29
        || text.get(3..5) != Some(b", ")
        || text.get(7) != Some(&b' ')
        || text.get(11) != Some(&b' ')
        || text.get(16) != Some(&b' ')
        || text.get(25..) != Some(b" GMT")
    {
        return None;
    }
    match text.get(..3)? {
        b"Mon" | b"Tue" | b"Wed" | b"Thu" | b"Fri" | b"Sat" | b"Sun" => {}
        _ => return None,
    }
    let month = match text.get(8..11)? {
        b"Jan" => b"01",
        b"Feb" => b"02",
        b"Mar" => b"03",
        b"Apr" => b"04",
        b"May" => b"05",
        b"Jun" => b"06",
        b"Jul" => b"07",
        b"Aug" => b"08",
        b"Sep" => b"09",
        b"Oct" => b"10",
        b"Nov" => b"11",
        b"Dec" => b"12",
        _ => return None,
    };
    let mut date = *b"0000-00-00T00:00:00Z";
    for (from, to, width) in [(12usize, 0usize, 4usize), (5, 8, 2), (17, 11, 8)] {
        for (input, output) in
            text.get(from..from.checked_add(width)?)?.iter().zip(date.get_mut(to..to.checked_add(width)?)?.iter_mut())
        {
            *output = *input;
        }
    }
    for (output, input) in date.get_mut(5..7)?.iter_mut().zip(month.iter()) {
        *output = *input;
    }
    read(&date)
}
