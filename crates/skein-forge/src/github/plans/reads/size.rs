//! Checked branch-list admission and the running allocation measure.
use super::{Bounds, api};
use crate::{github::pin, measure};

pub(super) fn capacity(bounds: Bounds) -> Result<u32, api::Error> {
    let Ok(width) = u32::try_from(measure::STRING_BYTES) else {
        return Err(api::Error::TooLarge);
    };
    let room = bounds.answer_bytes.checked_div(width).ok_or(api::Error::TooLarge)?;
    Ok(bounds.rows.min(room))
}
pub(super) fn page_size(bounds: Bounds, capacity: u32) -> Result<u32, api::Error> {
    let limit = bounds.page_size.min(capacity).min(pin::PAGE_CAP);
    if limit == 0 {
        return Err(api::Error::TooLarge);
    }
    Ok(limit)
}
pub(super) fn add(size: u64, name: &[u8], bounds: Bounds) -> Result<u64, api::Error> {
    let Ok(length) = u64::try_from(name.len()) else {
        return Err(api::Error::TooLarge);
    };
    let Ok(width) = u64::try_from(measure::STRING_BYTES) else {
        return Err(api::Error::TooLarge);
    };
    let total = size.checked_add(length).ok_or(api::Error::TooLarge)?.checked_add(width).ok_or(api::Error::TooLarge)?;
    if total > u64::from(bounds.answer_bytes) {
        return Err(api::Error::TooLarge);
    }
    Ok(total)
}
