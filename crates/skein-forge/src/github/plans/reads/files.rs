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
        if self.stage == Stage::Before || self.stage == Stage::After {
            if more {
                return Err(api::Error::TooLarge);
            }
            if u64::try_from(bytes.len()).ok().ok_or(api::Error::TooLarge)? > u64::from(self.raw_cap) {
                return Err(api::Error::TooLarge);
            }
            self.keep_bytes(0, &[&bytes], bounds)?;
            return self.side_done(Some(bytes), bounds);
        }
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

#[derive(Debug)]
pub(super) struct State {
    descriptors: skein_lib::Queue<crate::github::document::File>,
    files: skein_lib::List<api::File>,
    commits: skein_lib::List<api::Commit>,
    current: Option<api::File>,
    before: api::Commit,
    after: api::Commit,
    contains: bool,
}
impl State {
    pub(super) fn new() -> Self {
        Self {
            descriptors: skein_lib::Queue::with_capacity(0),
            files: skein_lib::List::with_capacity(0),
            commits: skein_lib::List::with_capacity(0),
            current: None,
            before: [0; 32],
            after: [0; 32],
            contains: false,
        }
    }
}
impl Plan {
    pub(super) fn files_admission(&mut self, bounds: Bounds) -> Result<(), api::Error> {
        let commits = super::size::capacity(bounds, crate::measure::COMMIT_BYTES)?;
        let files = super::size::capacity(bounds, crate::measure::FILE_BYTES)?;
        self.limit = super::size::page_size(bounds, commits)?;
        self.files.descriptors = skein_lib::Queue::with_capacity(files);
        self.files.files = skein_lib::List::with_capacity(files);
        self.files.commits = skein_lib::List::with_capacity(commits);
        Ok(())
    }
    pub(super) fn compare_initial(&mut self) -> Result<Step, api::Error> {
        let Read::Compare { before, after } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        self.files.before = before;
        self.files.after = after;
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let request =
            Request::Compare { owner: row.owner.clone(), name: row.name.clone(), before, after, per_page: self.limit };
        self.stage = Stage::Compare;
        Ok(self.send(request))
    }
    pub(super) fn file_row(&mut self, row: crate::github::document::Row, bounds: Bounds) -> Result<(), api::Error> {
        if self.stage != Stage::Compare {
            return Err(api::Error::InvalidAnswer);
        }
        match row {
            crate::github::document::Row::Commit(commit) => {
                if self.files.commits.len() >= self.limit || self.files.commits.len() >= bounds.rows {
                    return Err(api::Error::TooLarge);
                }
                self.keep_bytes(32, &[], bounds)?;
                if self.files.commits.push(commit).is_err() {
                    return Err(api::Error::TooLarge);
                }
            }
            crate::github::document::Row::File(file) => {
                if self.files.descriptors.len() >= bounds.rows || self.files.descriptors.len() >= 299 {
                    return Err(api::Error::TooLarge);
                }
                self.keep_bytes(64, &[&file.path], bounds)?;
                if self.files.descriptors.try_push(file).is_err() {
                    return Err(api::Error::TooLarge);
                }
            }
            crate::github::document::Row::Installation(_)
            | crate::github::document::Row::Reachable(_)
            | crate::github::document::Row::Branch(_)
            | crate::github::document::Row::Issue(_)
            | crate::github::document::Row::Comment(_)
            | crate::github::document::Row::Pull(_)
            | crate::github::document::Row::Context(_)
            | crate::github::document::Row::Run(_)
            | crate::github::document::Row::Collaborator(_)
            | crate::github::document::Row::Remark(_)
            | crate::github::document::Row::Review(_) => return Err(api::Error::InvalidAnswer),
        }
        Ok(())
    }
    pub(super) fn compare_done(
        &mut self,
        comparison: crate::github::document::Compare,
        bounds: Bounds,
    ) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Compare {
            return Err(api::Error::InvalidAnswer);
        }
        if comparison.total_commits > u64::from(self.files.commits.len()) {
            return Err(api::Error::TooLarge);
        }
        self.files.contains = comparison.behind_by == 0;
        self.next_file(bounds)
    }
    fn next_file(&mut self, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        if let Some(file) = self.files.descriptors.pop() {
            self.files.current = Some(api::File { path: file.path, before: None, after: None });
            return self.side_request(true, bounds);
        }
        let Read::Compare { before, after } = self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        let files = core::mem::replace(&mut self.files.files, skein_lib::List::with_capacity(0)).into_boxed();
        let commits = core::mem::replace(&mut self.files.commits, skein_lib::List::with_capacity(0)).into_boxed();
        self.terminal(
            api::Answer::Compare { before, after, contains_before: self.files.contains, files, commits },
            bounds,
        )
    }
    fn side_request(&mut self, before: bool, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        let file = self.files.current.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let request = Request::Raw {
            owner: row.owner.clone(),
            name: row.name.clone(),
            path: file.path.clone(),
            commit: if before { self.files.before } else { self.files.after },
        };
        self.stage = if before { Stage::Before } else { Stage::After };
        self.raw_cap =
            u32::try_from(u64::from(bounds.answer_bytes).checked_sub(self.size).ok_or(api::Error::TooLarge)?)
                .ok()
                .ok_or(api::Error::TooLarge)?;
        Ok(Some(self.send_raw(request, Retention::Whole { bytes: self.raw_cap })))
    }
    pub(super) fn side_done(&mut self, bytes: Option<Box<[u8]>>, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        let file = self.files.current.as_mut().ok_or(api::Error::InvalidAnswer)?;
        if self.stage == Stage::Before {
            file.before = bytes;
            return self.side_request(false, bounds);
        }
        if self.stage != Stage::After {
            return Err(api::Error::InvalidAnswer);
        }
        file.after = bytes;
        let file = self.files.current.take().ok_or(api::Error::InvalidAnswer)?;
        if self.files.files.push(file).is_err() {
            return Err(api::Error::TooLarge);
        }
        self.next_file(bounds)
    }
}
