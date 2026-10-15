//! Session records, encoded under an application's prefix (ui-web.md, 10).
use crate::platform::Platform;
use skein_lib::{
    Writer,
    base64::{self, Alphabet},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub key: Box<[u8]>,
    pub value: Box<[u8]>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Refused;
pub struct Store {
    prefix: Box<[u8]>,
    bound: Option<u64>,
}
impl Store {
    #[must_use]
    pub fn new(prefix: &[u8], bound: Option<u64>) -> Self {
        Self { prefix: prefix.into(), bound }
    }
    #[must_use]
    pub fn read_all(&self, platform: &impl Platform) -> Vec<Record> {
        platform
            .read_all()
            .into_iter()
            .filter_map(|record| {
                let key = record.key.strip_prefix(self.prefix.as_ref())?;
                Some(Record {
                    key: base64::decode(Alphabet::Standard, key, u32::MAX).ok()?,
                    value: base64::decode(Alphabet::Standard, &record.value, u32::MAX).ok()?,
                })
            })
            .collect()
    }
    pub fn write(&mut self, key: &[u8], value: &[u8], platform: &mut impl Platform) -> Result<(), Refused> {
        let key = self.key(key)?;
        let value = encoded(value)?;
        // The bound prices the text bytes held under this prefix, including keys.
        let held = platform
            .read_all()
            .into_iter()
            .filter(|record| record.key.starts_with(&self.prefix) && record.key != key)
            .try_fold(0_u64, |total, record| total.checked_add(size(&record.key, &record.value)?));
        let total = held.and_then(|held| held.checked_add(size(&key, &value)?)).ok_or(Refused)?;
        if self.bound.is_some_and(|bound| total > bound) || !platform.write(&key, &value) {
            return Err(Refused);
        }
        Ok(())
    }
    pub fn remove(&mut self, key: &[u8], platform: &mut impl Platform) {
        if let Ok(key) = self.key(key) {
            platform.remove(&key);
        }
    }
    fn key(&self, key: &[u8]) -> Result<Box<[u8]>, Refused> {
        let mut text = self.prefix.to_vec();
        text.extend_from_slice(&encoded(key)?);
        Ok(text.into())
    }
}
fn size(key: &[u8], value: &[u8]) -> Option<u64> {
    u64::try_from(key.len()).ok()?.checked_add(u64::try_from(value.len()).ok()?)
}
fn encoded(bytes: &[u8]) -> Result<Box<[u8]>, Refused> {
    let len =
        base64::encoded_len(Alphabet::Standard, u32::try_from(bytes.len()).map_err(|_| Refused)?).ok_or(Refused)?;
    let mut out = Writer::new(usize::try_from(len).map_err(|_| Refused)?);
    base64::encode(Alphabet::Standard, bytes, &mut out).map_err(|_| Refused)?;
    Ok(out.finish())
}
