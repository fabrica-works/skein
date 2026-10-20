//! REST refusal is the only entrance to queue admission (forge-github.md, 5.1).
use super::{Plan, Request, Stage, Step, api};
use crate::github::document;
impl Plan {
    pub(super) fn merge_pull(&mut self) -> Result<Step, api::Error> {
        let api::Write::Merge { number, .. } = self.write else {
            return Err(api::Error::InvalidAnswer);
        };
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        self.stage = Stage::PullGraphql;
        Ok(self.send(Request::PullGraphql { owner: row.owner.clone(), name: row.name.clone(), number }))
    }
    pub(super) fn enqueue(&mut self, pull: document::PullGraphql) -> Result<Step, api::Error> {
        let api::Write::Merge { head, .. } = self.write else {
            return Err(api::Error::InvalidAnswer);
        };
        if pull.id.is_empty() {
            return Err(self.invalid());
        }
        self.stage = Stage::Enqueue;
        Ok(self.send(Request::Enqueue { id: pull.id, head }))
    }
}
