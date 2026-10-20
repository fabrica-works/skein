//! Raw file retention is an instruction on the dialect's exchange.
use super::{Bounds, Plan, Read, Request, Response, Stage, Step, api};
use crate::github::plans::plan::{Exchange, Retention};
use alloc::boxed::Box;
impl Plan {
    pub(super) fn send_raw(&mut self, request: Request, retention: Retention) -> Step {
        self.response = Response::Head;
        self.count = 0;
        Step::Send(Exchange { request, retention: Some(retention) })
    }
    pub(super) fn file_initial(&mut self) -> Result<Step, api::Error> {
        let Read::File { head, path, .. } = &self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let request =
            Request::Raw { owner: row.owner.clone(), name: row.name.clone(), path: path.clone(), commit: *head };
        self.stage = Stage::Raw;
        Ok(self.send_raw(request, Retention::Head { bytes: self.raw_cap }))
    }
    pub(super) fn raw_done(
        &mut self,
        bytes: Box<[u8]>,
        more: bool,
        bounds: Bounds,
    ) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Raw {
            return Err(api::Error::InvalidAnswer);
        }
        let Read::File { head, path, .. } = &self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        if u64::try_from(bytes.len()).ok().ok_or(api::Error::TooLarge)? > u64::from(self.raw_cap) {
            return Err(api::Error::TooLarge);
        }
        let answer = api::Answer::File { head: *head, path: path.clone(), bytes, truncated: more };
        self.terminal(answer, bounds)
    }
}
