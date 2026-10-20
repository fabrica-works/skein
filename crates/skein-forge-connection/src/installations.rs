//! One GitHub forge's bounded installation state (forge-connection.md, 4.4 and 5.3).
use crate::{Account, app::Jwt};
use skein_forge::github::request::Scope;
use skein_lib::{Map, Secret, Set, Wall};

#[derive(Debug)]
pub(crate) struct Row {
    pub account: Account,
    pub suspended: bool,
    pub accepted: bool,
    pub api: Option<Secret>,
    pub api_expires: Wall,
    pub git_expires: Wall,
}
impl Row {
    pub fn forget(&mut self) {
        self.api = None;
        self.api_expires = Wall::EPOCH;
        self.git_expires = Wall::EPOCH;
    }
}
#[derive(Debug)]
pub(crate) struct GitHub {
    pub jwt: Jwt,
    pub accepted: Set<u64>,
    pub rows: Map<u64, Row>,
    pub mint: Option<(u64, Scope)>,
    pub stale: bool,
    pub recent: Option<u64>,
    pub identity_checked: bool,
    pub status: Option<u16>,
    pub retry: Option<skein_lib::Duration>,
}
impl GitHub {
    pub fn new(cap: u32) -> Self {
        Self {
            jwt: Jwt::new(),
            accepted: Set::with_capacity(cap),
            rows: Map::with_capacity(cap),
            mint: None,
            stale: false,
            recent: None,
            identity_checked: false,
            status: None,
            retry: None,
        }
    }
    pub fn forget(&mut self) {
        self.jwt.forget();
        self.rows = Map::with_capacity(self.rows.capacity());
        self.mint = None;
        self.stale = false;
        self.recent = None;
        self.identity_checked = false;
        self.status = None;
        self.retry = None;
    }
    pub fn accept(&mut self, ids: &[u64]) {
        let mut accepted = Set::with_capacity(self.accepted.capacity());
        for id in ids {
            if *id != 0 {
                let _inserted = accepted.insert(*id);
            }
        }
        let changed = accepted.iter().ne(self.accepted.iter());
        self.accepted = accepted;
        #[expect(clippy::manual_map, reason = "the programming model excludes closures")]
        let mint_id = match self.mint {
            Some((id, _)) => Some(id),
            None => None,
        };
        let mut ids = skein_lib::List::with_capacity(self.rows.capacity());
        for (id, _) in &self.rows {
            ids.push(*id).expect("bounded rows");
        }
        for id in ids.into_boxed() {
            let row = self.rows.get_mut(&id).expect("row");
            row.accepted = self.accepted.contains(&id);
            if !row.accepted || (changed && mint_id == Some(id)) {
                row.forget();
            }
        }
        if changed && self.mint.is_some() {
            self.stale = true;
        }
        if let Some(id) = self.recent
            && !self.accepted.contains(&id)
        {
            self.recent = None;
        }
    }
}
