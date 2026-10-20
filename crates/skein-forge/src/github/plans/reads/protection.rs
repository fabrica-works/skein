//! Both GitHub protection sources are joined; neither absence hides the other.
use super::{Bounds, Plan, Read, Request, Response, Stage, Step, api, document, size};
use alloc::boxed::Box;
use skein_lib::List;
#[derive(Debug)]
pub(super) struct State {
    ids: List<u64>,
    at: u32,
    contexts: List<Box<[u8]>>,
    value: api::Protection,
    kept: bool,
}
impl State {
    pub(super) fn new() -> Self {
        Self {
            ids: List::with_capacity(0),
            at: 0,
            contexts: List::with_capacity(0),
            kept: false,
            value: api::Protection {
                branch: Box::from([]),
                contexts: Box::from([]),
                approvals: 0,
                dismiss_stale: false,
                code_owners: false,
                linear: false,
                signatures: false,
                queue: false,
                bypass: api::Bypass::Always,
            },
        }
    }
}
impl Plan {
    pub(super) fn protection_admission(&mut self, bounds: Bounds) -> Result<(), api::Error> {
        let Read::Protection { branch } = &self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        self.protection.value.branch = branch.clone();
        self.protection.ids = List::with_capacity(bounds.inner);
        self.protection.contexts = List::with_capacity(size::capacity(bounds, crate::measure::STRING_BYTES)?);
        self.size = u64::try_from(branch.len()).or(Err(api::Error::TooLarge))?;
        if self.size > u64::from(bounds.answer_bytes) {
            return Err(api::Error::TooLarge);
        }
        Ok(())
    }
    pub(super) fn protection_initial(&mut self) -> Result<Step, api::Error> {
        let Read::Protection { branch } = &self.read else {
            return Err(api::Error::InvalidAnswer);
        };
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let owner = row.owner.clone();
        let name = row.name.clone();
        self.stage = Stage::Rules;
        Ok(self.send(Request::RulesForBranch { owner, name, branch: branch.clone(), page: self.page }))
    }
    fn protection_next(&mut self) -> Result<Option<Step>, api::Error> {
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let owner = row.owner.clone();
        let name = row.name.clone();
        let request = match self.protection.ids.get(self.protection.at) {
            Some(id) => {
                self.stage = Stage::Ruleset;
                Request::Ruleset { owner, name, id: *id }
            }
            None => {
                let Read::Protection { branch } = &self.read else {
                    return Err(api::Error::InvalidAnswer);
                };
                self.stage = Stage::Classic;
                Request::ClassicProtection { owner, name, branch: branch.clone() }
            }
        };
        Ok(Some(self.send(request)))
    }
    fn protection_contexts(&mut self, contexts: Box<[Box<[u8]>]>, bounds: Bounds) -> Result<(), api::Error> {
        for context in contexts {
            if self.protection.contexts.as_slice().contains(&context) {
                continue;
            }
            self.size = size::add(self.size, &context, bounds)?;
            self.protection.contexts.push(context).or(Err(api::Error::TooLarge))?;
        }
        Ok(())
    }
    pub(super) fn protection_row(&mut self, row: document::Row, bounds: Bounds) -> Result<(), api::Error> {
        if self.stage != Stage::Rules {
            return Err(api::Error::InvalidAnswer);
        }
        let document::Row::Rule(row) = row else {
            return Err(api::Error::InvalidAnswer);
        };
        self.count = self.count.checked_add(1).ok_or(api::Error::TooLarge)?;
        if self.count > crate::github::pin::PAGE_CAP {
            return Err(api::Error::TooLarge);
        }
        if row.kind == document::RuleKind::Skipped {
            return Ok(());
        }
        if !self.protection.ids.as_slice().contains(&row.ruleset) {
            self.protection.ids.push(row.ruleset).or(Err(api::Error::TooLarge))?;
        }
        self.protection.kept = true;
        match row.kind {
            document::RuleKind::PullRequest { approvals, dismiss_stale, code_owners } => {
                self.protection.value.approvals = self.protection.value.approvals.max(approvals);
                self.protection.value.dismiss_stale |= dismiss_stale;
                self.protection.value.code_owners |= code_owners;
            }
            document::RuleKind::StatusChecks(contexts) => self.protection_contexts(contexts, bounds)?,
            document::RuleKind::Queue => self.protection.value.queue = true,
            document::RuleKind::Signatures => self.protection.value.signatures = true,
            document::RuleKind::Linear => self.protection.value.linear = true,
            document::RuleKind::Skipped => {}
        }
        Ok(())
    }
    pub(super) fn protection_rows_done(&mut self) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Rules {
            return Err(api::Error::InvalidAnswer);
        }
        let Response::Body { next } = self.response else {
            return Err(api::Error::InvalidAnswer);
        };
        if next {
            self.page = self.page.checked_add(1).ok_or(api::Error::TooLarge)?;
            return Ok(Some(self.protection_initial()?));
        }
        self.protection_next()
    }
    pub(super) fn ruleset_done(&mut self, bypass: api::Bypass) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Ruleset {
            return Err(api::Error::InvalidAnswer);
        }
        self.protection.value.bypass = self.protection.value.bypass.min(bypass);
        self.protection.at = self.protection.at.checked_add(1).ok_or(api::Error::TooLarge)?;
        self.protection_next()
    }
    pub(super) fn classic_done(
        &mut self,
        value: Option<document::Protection>,
        bounds: Bounds,
    ) -> Result<Option<Step>, api::Error> {
        if self.stage != Stage::Classic {
            return Err(api::Error::InvalidAnswer);
        }
        if let Some(value) = value {
            self.protection.kept = true;
            self.protection.value.bypass = api::Bypass::Never;
            self.protection.value.approvals = self.protection.value.approvals.max(value.approvals);
            self.protection.value.dismiss_stale |= value.dismiss_stale;
            self.protection.value.code_owners |= value.code_owners;
            self.protection.value.linear |= value.linear;
            self.protection.value.signatures |= value.signatures;
            self.protection_contexts(value.contexts, bounds)?;
        }
        let value = if self.protection.kept {
            self.protection.value.contexts =
                core::mem::replace(&mut self.protection.contexts, List::with_capacity(0)).into_boxed();
            Some(core::mem::replace(&mut self.protection.value, State::new().value))
        } else {
            None
        };
        self.terminal(api::Answer::Protection(value), bounds)
    }
}
