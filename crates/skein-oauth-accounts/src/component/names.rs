//! Stable account numbers and their bounded places (oauth.md, section 6.2).

use super::{Component, Lifecycle, Name};
use crate::{Asked, Event, Refusal, Request};
use skein_lib::{List, Queue};
use skein_oauth::{Registration, SavedToken};

impl Component {
    fn place(&self, account: u32) -> Option<u32> {
        for place in 0..self.names.len() {
            match self.names.get(place).expect("bounded names") {
                Name::Live { number, .. } if *number == account => return Some(place),
                Name::Live { .. } | Name::Removing { .. } | Name::Free => {}
            }
        }
        None
    }

    pub(super) fn named(&self, request: Request, up: &mut Queue<Event>) -> Option<Request> {
        Some(match request {
            Request::Want { account, wanted } => {
                Request::Want { account: self.lookup(account, Asked::Want, up)?, wanted }
            }
            Request::Remove { account } => Request::Remove { account: self.lookup(account, Asked::Remove, up)? },
            Request::SignOut { account } => Request::SignOut { account: self.lookup(account, Asked::SignOut, up)? },
            Request::SignIn { account } => Request::SignIn { account: self.lookup(account, Asked::SignIn, up)? },
            Request::Redirected { account, uri } => {
                Request::Redirected { account: self.lookup(account, Asked::Redirected, up)?, uri }
            }
            Request::HandIn { account, record } => {
                Request::HandIn { account: self.lookup(account, Asked::HandIn, up)?, record }
            }
            Request::Grant { account } => Request::Grant { account: self.lookup(account, Asked::Grant, up)? },
            Request::Cancel { account } => Request::Cancel { account: self.place(account)? },
            Request::Rejected { account, generation } => {
                Request::Rejected { account: self.place(account)?, generation }
            }
            Request::Release { account } => Request::Release { account: self.place(account)? },
            Request::Kept { account, generation, keeping } => {
                Request::Kept { account: self.terminal_place(account)?, generation, keeping }
            }
            Request::Close => Request::Close,
            Request::Abort => Request::Abort,
        })
    }

    fn lookup(&self, account: u32, asked: Asked, up: &mut Queue<Event>) -> Option<u32> {
        match self.place(account) {
            Some(place) => Some(place),
            None => {
                let why = match self.lifecycle {
                    Lifecycle::Live => Refusal::Account,
                    Lifecycle::Closing | Lifecycle::Aborting | Lifecycle::Closed => Refusal::Closed,
                };
                up.push(Event::Refused { account, asked, why });
                None
            }
        }
    }
}

pub(super) fn number(names: &List<Name>, place: u32) -> u32 {
    match names.get(place).expect("account name") {
        Name::Live { number, .. } | Name::Removing { number } => *number,
        Name::Free => unreachable!("free account emits nothing"),
    }
}

pub(super) fn missing_id(registration: &Registration, record: &SavedToken) -> bool {
    match &registration.terms {
        Some(terms) => {
            terms.issued_client_id
                && match &record.issued_client_id {
                    Some(id) => id.is_empty(),
                    None => true,
                }
        }
        None => false,
    }
}

#[cfg(test)]
impl Component {
    pub(crate) fn private_expected(&self, account: u32) -> skein_io::file::Expect {
        match self.keepers.get(account).expect("account keeper") {
            crate::keeper::Store::Private(keeper) => keeper.expected,
            crate::keeper::Store::Owner => unreachable!("private keeper fixture"),
        }
    }
}
