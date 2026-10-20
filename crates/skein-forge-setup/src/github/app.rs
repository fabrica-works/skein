//! The App and failures shared by GitHub's plans (forge-setup.md, 6.4).
use crate::{
    call::Auth,
    item,
    reconcile::{Failure, Need, Stage, Why},
};
use alloc::boxed::Box;
use core::num::NonZeroU64;
use skein_lib::{Decimal, Secret, bytes};

/// The App a plan acts as: its id and its key (forge-setup.md, 6.4).
#[derive(Clone, Debug)]
pub(crate) struct Kept {
    id: u64,
    key: Secret,
}
impl Kept {
    pub(crate) fn new(id: NonZeroU64, key: Secret) -> Self {
        Self { id: id.get(), key }
    }
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
    /// The host signs a fresh JWT with the key for each send.
    pub(crate) fn auth(&self) -> Auth {
        Auth::App { id: self.id(), key: self.key.clone() }
    }
}
/// An absent or refused App id is no id.
pub(crate) fn id_of(read: Option<Secret>) -> Option<NonZeroU64> {
    item::app_id(read?.expose())
}
/// A plan's stop, naming its stage and subject without credentials.
pub(crate) fn stop(forge: u16, stage: Stage, subject: &[u8], why: Why) -> Need {
    Need::Stop(Failure { forge, step: stage, subject: bytes::copy_of(subject), why })
}
/// An installation id as the subject of a failure.
pub(crate) fn decimal(id: u64) -> Box<[u8]> {
    bytes::copy_of(Decimal::of(id).as_bytes())
}
