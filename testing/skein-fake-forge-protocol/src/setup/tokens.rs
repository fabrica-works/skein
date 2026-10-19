use super::{Issued, Store, encoded, refusal};
use crate::{Config, Limits, response::Reply};
use alloc::boxed::Box;
use skein_forge::forgejo::{document::AccessToken, request::Page, server::Answer};
use skein_lib::{Decimal, List, Secret, Writer, bytes};
impl Store {
    /// Adds an issued scenario token, within the document's name and scope bounds.
    pub fn token(&mut self, user: u64, name: &[u8], scopes: &[&[u8]], secret: Secret) {
        let mut owned = List::with_capacity(u32::try_from(scopes.len()).expect("scope count"));
        for scope in scopes {
            owned.push(bytes::copy_of(scope)).expect("scope capacity");
        }
        self.issue(user, name, owned.into_boxed(), secret);
    }
    fn issue(&mut self, user: u64, name: &[u8], scopes: Box<[Box<[u8]>]>, secret: Secret) -> u64 {
        assert!(
            name.len() <= self.string_bytes
                && scopes.len() <= usize::try_from(self.tokens.capacity()).expect("u32 fits")
                && secret.len() <= self.secret_bytes.max(40),
            "scenario token bounds"
        );
        for scope in &scopes {
            assert!(scope.len() <= self.string_bytes, "scenario scope bound");
        }
        let id = self.next_token;
        self.next_token = id.checked_add(1).expect("scenario token ids");
        self.tokens
            .push(Issued { id, user, name: bytes::copy_of(name), scopes, secret })
            .expect("tokens bounded by documents.inner");
        id
    }
    pub(super) fn list_tokens(&self, login: &[u8], page: Page, config: &Config, limits: &Limits) -> Reply {
        let Some(account) = self.find(login) else {
            return refusal(404, b"The target couldn't be found.", limits);
        };
        let (page, cap) = crate::routes::page(config, page);
        let start = page.saturating_sub(1).saturating_mul(cap);
        let mut rows = List::with_capacity(cap);
        let mut index = 0_u32;
        for token in &self.tokens {
            if token.user == account.id {
                if index >= start && rows.room() > 0 {
                    rows.push(document(token, false)).expect("page capacity");
                }
                index = index.saturating_add(1);
            }
        }
        encoded(200, Answer::Tokens(rows.into_boxed()), limits)
    }
    pub(super) fn create_token(&mut self, login: &[u8], name: &[u8], scopes: &[Box<[u8]>], limits: &Limits) -> Reply {
        let Some(account) = self.find(login) else {
            return refusal(404, b"The target couldn't be found.", limits);
        };
        let user = account.id;
        if self.scopes(user, name).is_some() {
            return refusal(400, b"access token name has been used already", limits);
        }
        if self.tokens.room() == 0 {
            return refusal(503, b"setup store is full", limits);
        }
        let digits = Decimal::of(self.next_token);
        let mut value = Writer::new(40);
        for _ in digits.as_bytes().len()..40 {
            value.put(b"0").expect("40 digit token");
        }
        value.put(digits.as_bytes()).expect("token id fits 40 digits");
        self.issue(user, name, Box::from(scopes), Secret::new(value.finish()));
        encoded(201, Answer::Token(document(self.tokens.last().expect("token issued"), true)), limits)
    }
    pub(super) fn delete_token(&mut self, login: &[u8], id: u64, limits: &Limits) -> Reply {
        let Some(account) = self.find(login) else {
            return refusal(404, b"The target couldn't be found.", limits);
        };
        let user = account.id;
        let old = core::mem::replace(&mut self.tokens, List::with_capacity(limits.documents.inner));
        let mut found = false;
        for token in old.into_boxed() {
            if token.id == id && token.user == user {
                found = true;
            } else {
                self.tokens.push(token).expect("same token capacity");
            }
        }
        if found {
            Reply { status: 204, headers: Box::new([]), body: Box::new([]) }
        } else {
            refusal(404, b"The target couldn't be found.", limits)
        }
    }
}
fn document(token: &Issued, value: bool) -> AccessToken {
    AccessToken {
        id: token.id,
        name: token.name.clone(),
        scopes: token.scopes.clone(),
        last_eight: bytes::copy_of(
            token.secret.expose().get(token.secret.len().saturating_sub(8)..).expect("token suffix"),
        ),
        value: if value { Some(token.secret.clone()) } else { None },
    }
}
