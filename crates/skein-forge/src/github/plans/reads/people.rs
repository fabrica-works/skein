//! User resolution precedes a permission request only on a users-table miss.
use super::{Bounds, Plan, Read, Request, Stage, Step, Tables, api, document};

impl Plan {
    pub(super) fn permission_initial(&mut self) -> Result<Step, api::Error> {
        let Read::Permission { user } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        let request = match &self.login {
            Some(login) => {
                let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
                self.stage = Stage::Permission;
                Request::Permission { owner: row.owner.clone(), name: row.name.clone(), login: login.clone() }
            }
            None => {
                self.stage = Stage::ResolveUser;
                Request::User { id: user }
            }
        };
        Ok(self.send(request))
    }
    pub(super) fn user_done(&mut self, user: document::User, tables: &mut Tables) -> Result<Option<Step>, api::Error> {
        let Read::Permission { user: expected } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        if self.stage != Stage::ResolveUser || user.id != expected {
            return Err(api::Error::InvalidAnswer);
        }
        tables.keep_user(self.repository.forge, user.id, user.login.clone());
        self.login = Some(user.login);
        Ok(Some(self.permission_initial()?))
    }
    pub(super) fn permission_done(
        &mut self,
        value: api::Permission,
        bounds: Bounds,
    ) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Permission {
            return Err(api::Error::InvalidAnswer);
        }
        self.terminal(api::Answer::Permission(value), bounds)
    }
}

impl Plan {
    pub(super) fn collaborators_initial(&mut self) -> Result<Step, api::Error> {
        let Read::Collaborators { page } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let request =
            Request::Collaborators { owner: row.owner.clone(), name: row.name.clone(), page, per_page: self.limit };
        self.stage = Stage::Read;
        Ok(self.send(request))
    }
    pub(super) fn collaborator_row(
        &mut self,
        row: document::Row,
        bounds: Bounds,
        tables: &mut Tables,
    ) -> Result<(), api::Error> {
        if self.stage != Stage::Read {
            return Err(api::Error::InvalidAnswer);
        }
        let document::Row::Collaborator(row) = row else {
            return Err(api::Error::InvalidAnswer);
        };
        self.count = self.count.checked_add(1).ok_or(api::Error::TooLarge)?;
        if self.count > self.limit || self.collaborators.len() >= bounds.rows {
            return Err(api::Error::TooLarge);
        }
        self.size = self.size.checked_add(16).ok_or(api::Error::TooLarge)?;
        if self.size > u64::from(bounds.answer_bytes) {
            return Err(api::Error::TooLarge);
        }
        if let Some(login) = row.login {
            tables.keep_user(self.repository.forge, row.id, login);
        }
        match self.collaborators.push(api::Collaborator { user: row.id, permission: row.permission }) {
            Ok(()) => Ok(()),
            Err(_) => Err(api::Error::TooLarge),
        }
    }
    pub(super) fn collaborators_done(&mut self, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Read {
            return Err(api::Error::InvalidAnswer);
        }
        let more = match self.response {
            super::Response::Body { next } => next,
            super::Response::None | super::Response::Head | super::Response::Refusal { .. } => {
                return Err(api::Error::InvalidAnswer);
            }
        };
        let collaborators = core::mem::replace(&mut self.collaborators, skein_lib::List::with_capacity(0)).into_boxed();
        self.terminal(api::Answer::Collaborators { collaborators, more }, bounds)
    }
}
