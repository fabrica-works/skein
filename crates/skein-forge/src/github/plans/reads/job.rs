//! One attempt's metadata, followed by its log redirect (forge-github.md, 10).
use super::{Bounds, Plan, Read, Request, Response, Stage, Step, api, document};
use crate::github::{header::Head, plans::plan::Retention, redirect};
use alloc::boxed::Box;

impl Plan {
    pub(super) fn job_document(&mut self, job: document::Job, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        if let Read::Job { .. } = self.read { self.job_done(job, bounds) } else { self.checks_job_done(job, bounds) }
    }
    pub(super) fn job_initial(&mut self) -> Result<Step, api::Error> {
        let Read::Job { attempt, .. } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let request = Request::Job { owner: row.owner.clone(), name: row.name.clone(), id: attempt.job };
        self.stage = Stage::Job;
        Ok(self.send(request))
    }
    pub(super) fn job_done(&mut self, job: document::Job, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        let Read::Job { attempt, max_bytes } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        if self.stage != Stage::Job {
            return Err(api::Error::InvalidAnswer);
        }
        if job.id != attempt.job || job.run != attempt.run || job.attempt != attempt.attempt || job.head != attempt.head
        {
            return Err(api::Error::MissingJob);
        }
        let step_bytes = match &job.step {
            Some(step) => u32::try_from(step.name.len()).or(Err(api::Error::TooLarge))?,
            None => 0,
        };
        self.raw_cap = max_bytes.min(bounds.answer_bytes.checked_sub(step_bytes).ok_or(api::Error::TooLarge)?);
        self.job_step = job.step;
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let request = Request::JobLog { owner: row.owner.clone(), name: row.name.clone(), id: attempt.job };
        self.stage = Stage::JobLog;
        Ok(Some(self.send(request)))
    }
    pub(super) fn job_redirect(&mut self, head: &Head) -> Result<Option<Step>, api::Error> {
        let location = head.location.as_ref().ok_or(api::Error::InvalidAnswer)?;
        // The header reader has already bounded Location. Neither parsed field
        // can exceed it (forge-github.md, 10).
        let length = u32::try_from(location.len()).or(Err(api::Error::InvalidAnswer))?;
        let redirect = redirect::parse(location, length, length)?;
        self.stage = Stage::Log;
        self.response = Response::Head;
        Ok(Some(Step::Redirect { redirect, retention: Retention::Tail { bytes: self.raw_cap } }))
    }
    pub(super) fn job_raw(&mut self, bytes: Box<[u8]>, more: bool, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        let Read::Job { attempt, .. } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        if self.stage != Stage::Log {
            return Err(api::Error::InvalidAnswer);
        }
        let cap = usize::try_from(self.raw_cap).expect("u32 fits usize");
        let start = bytes.len().saturating_sub(cap);
        let truncated = more || start != 0;
        let log =
            if start == 0 { bytes } else { skein_lib::bytes::copy_of(bytes.get(start..).expect("tail within bytes")) };
        let step = self.job_step.take();
        self.terminal(api::Answer::Job { attempt, step, log, truncated }, bounds)
    }
}
