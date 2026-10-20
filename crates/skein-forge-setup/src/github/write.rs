//! The four durable identity items shared by conversion and paste (forge-setup.md, 5.2, 5.8, 6.4).
use crate::{
    item::Item,
    reconcile::{Event, Need, Why},
};
use alloc::boxed::Box;
use skein_lib::{Secret, bytes};

#[derive(Debug)]
enum State {
    Begin,
    AppId,
    AppSlug,
    AppKey,
    User,
    Ended,
}
/// Concrete identity progression: one whole item per acknowledgment (forge-setup.md, 5.8).
#[derive(Debug)]
pub(super) struct Write {
    state: State,
    id: u64,
    slug: Box<[u8]>,
    key: Option<Secret>,
    bot: u64,
}
impl Write {
    pub(super) fn new(id: u64, slug: Box<[u8]>, key: Secret, bot: u64) -> Self {
        Self { state: State::Begin, id, slug, key: Some(key), bot }
    }
    pub(super) fn advance(&mut self, event: Event) -> Result<Option<Need>, Why> {
        let state = core::mem::replace(&mut self.state, State::Ended);
        match state {
            State::Begin => match event {
                Event::Begin => (),
                Event::Read(_) | Event::Reply(_) | Event::Written | Event::Drawn(_) => return Err(Why::Unexpected),
            },
            State::AppId | State::AppSlug | State::AppKey | State::User => match event {
                Event::Written => (),
                Event::Begin | Event::Read(_) | Event::Reply(_) | Event::Drawn(_) => return Err(Why::Unexpected),
            },
            State::Ended => return Err(Why::Unexpected),
        }
        let (item, bytes, next) = match state {
            State::Begin => (Item::AppId, Secret::new(bytes::copy_of(&self.id.to_le_bytes())), State::AppId),
            State::AppId => (Item::AppSlug, Secret::new(self.slug.clone()), State::AppSlug),
            State::AppSlug => (Item::AppKey, self.key.take().expect("key held until its write"), State::AppKey),
            State::AppKey => (Item::User, Secret::new(bytes::copy_of(&self.bot.to_le_bytes())), State::User),
            State::User => return Ok(None),
            State::Ended => unreachable!("ended writes were refused"),
        };
        self.state = next;
        Ok(Some(Need::Write { item, bytes }))
    }
}
