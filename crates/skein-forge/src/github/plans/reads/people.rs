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
