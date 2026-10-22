//! Disclosures fold when the path or query moves between applies (ui-web.md, 7).

#[derive(Default)]
pub(crate) struct Fold {
    last: Option<Box<[u8]>>,
}

impl Fold {
    pub(crate) fn moved(&mut self, address: &[u8]) -> bool {
        if self.last.as_deref() == Some(address) {
            return false;
        }
        let moved = self.last.is_some();
        self.last = Some(address.into());
        moved
    }
}

#[cfg(test)]
#[path = "../tests/fold.rs"]
mod tests;
