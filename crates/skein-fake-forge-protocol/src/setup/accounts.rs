use super::{Account, Store, encoded, refusal};
use crate::{Limits, response::Reply};
use skein_forge::forgejo::server::Answer;
use skein_lib::bytes;
impl Store {
    /// Adds a scenario account; callers keep all fields within document bounds.
    pub fn account(&mut self, login: &[u8], admin: bool, full_name: &[u8]) -> u64 {
        assert!(login.len() <= self.string_bytes && full_name.len() <= self.string_bytes, "scenario account bounds");
        let id = self.next_account;
        self.next_account = id.checked_add(1).expect("scenario account ids");
        self.accounts
            .push(Account { id, login: bytes::copy_of(login), admin, full_name: bytes::copy_of(full_name) })
            .expect("accounts bounded by documents.inner");
        id
    }
    pub(super) fn create_account(&mut self, login: &[u8], full_name: &[u8], limits: &Limits) -> Reply {
        if self.find(login).is_some() {
            let message = super::joined(&[b"user already exists [name: ", login, b"]"]);
            return refusal(422, &message, limits);
        }
        if self.accounts.room() == 0 {
            return refusal(503, b"setup store is full", limits);
        }
        let id = self.account(login, false, full_name);
        encoded(201, Answer::User(self.user(id).expect("account made")), limits)
    }
}
