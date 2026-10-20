//! Puller work shares the forge's own connection (forge-connection.md, 6.5).
use super::{Component, OwnPlan};
use crate::{
    Account, Change, Event, Fact, Limits, Stopped,
    hooks::Hinted,
    installations::{Read, Work},
    puller::Action,
};
use skein_forge::api;
use skein_forge::github::{self, delivery::Delivery, plans::plan::Bounds};
use skein_lib::{Env, Queue};

impl Component {
    pub(super) fn github_log_bounds(l: &Limits, delivery: bool) -> Bounds {
        Bounds {
            page_size: github::pin::PAGE_CAP.min(l.inner),
            rows: github::pin::PAGE_CAP.min(l.inner),
            // Log pages are independent of calls and include the dialect's bounded strings.
            answer_bytes: if delivery {
                l.delivery_bytes
            } else {
                crate::limits::log_page_bytes(l).expect("checked log page")
            },
            body_bytes: l.body_bytes,
            inner: l.inner,
        }
    }
    pub(super) fn github_log_active(&self, at: u32) -> bool {
        match self.own_plans.get(at).expect("forge plan") {
            Some(OwnPlan::GitHubLogPage(_) | OwnPlan::GitHubLogDelivery(_)) => true,
            Some(
                OwnPlan::Forgejo(_)
                | OwnPlan::GitHubInstallations(_)
                | OwnPlan::GitHubMinting(_)
                | OwnPlan::GitHubIdentity(_),
            )
            | None => false,
        }
    }
    pub(super) fn github_log_selected(&self, at: u32) -> bool {
        if !super::ready(*self.starts.get(at).expect("forge start")) {
            return false;
        }
        let gh = self.github.get(at).expect("forge slot").as_ref().expect("GitHub state");
        let reading = match gh.read {
            Read::Reading(_) | Read::Changes(_) => true,
            Read::Idle | Read::Waiting { .. } => false,
        };
        let working = match gh.work {
            Work::Minting { .. } | Work::Identity { .. } => true,
            Work::Idle | Work::Waiting { .. } => false,
        };
        !reading && !working && gh.puller.wants_request()
    }
    pub(super) fn github_log_local(&mut self, at: u32, env: &Env<Limits>, up: &mut Queue<Event>) -> bool {
        let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
        let forge = self.settings.forges.get(at).expect("forge").number;
        match gh.puller.local(env) {
            Action::Idle => false,
            Action::Progress => true,
            Action::Installations => {
                gh.reread = true;
                true
            }
            Action::Gap { reread } => {
                gh.reread |= reread;
                up.push(Event::Fact { forge, fact: Fact::Gap });
                true
            }
            Action::Hint(hint) => {
                up.push(Event::Hint(Hinted { connection: gh.puller.source.expect("delivery source"), hint }));
                self.wake = Some(env.now);
                false
            }
        }
    }
    pub(super) fn github_log_failed(&mut self, at: u32, env: &Env<Limits>) {
        let delivery = match self.own_plans.get(at).expect("forge plan") {
            Some(OwnPlan::GitHubLogDelivery(_)) => true,
            Some(OwnPlan::GitHubLogPage(_)) => false,
            Some(
                OwnPlan::Forgejo(_)
                | OwnPlan::GitHubInstallations(_)
                | OwnPlan::GitHubMinting(_)
                | OwnPlan::GitHubIdentity(_),
            )
            | None => unreachable!("active log read"),
        };
        let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
        if delivery {
            gh.puller.reading();
        } else {
            gh.puller.finish(env, false);
        }
        *self.own_plans.get_mut(at).expect("forge plan") = None;
    }
    pub(super) fn github_log_error(&mut self, at: u32, env: &Env<Limits>, up: &mut Queue<Event>) {
        let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
        if gh.status == Some(401) {
            self.github_stop(at, Stopped::KeyRefused, up);
        } else {
            gh.puller.finish(env, false);
        }
    }
    pub(super) fn github_log_delivery(
        &mut self,
        at: u32,
        answer: Result<github::log::Logged, api::Error>,
        up: &mut Queue<Event>,
    ) {
        self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state").puller.reading();
        match answer {
            Ok(logged) => self.github_logged(at, logged, up),
            Err(_) => {
                if self.github.get(at).expect("forge slot").as_ref().expect("GitHub state").status == Some(401) {
                    self.github_stop(at, Stopped::KeyRefused, up);
                }
            }
        }
    }
    pub(super) fn github_logged(&mut self, at: u32, logged: github::log::Logged, up: &mut Queue<Event>) {
        let forge = self.settings.forges.get(at).expect("forge").number;
        match &logged.delivery {
            Delivery::Installation { installation, account, change } => {
                if *change == github::delivery::Change::Repositories {
                    let gh = self.github.get(at).expect("forge slot").as_ref().expect("GitHub state");
                    let account = match gh.rows.get(installation) {
                        Some(row) => row.account.clone(),
                        None => Account { id: account.id, login: account.login.clone() },
                    };
                    up.push(Event::Installation {
                        forge,
                        installation: *installation,
                        account: Some(account),
                        change: Change::Repositories,
                    });
                }
            }
            Delivery::Issue { .. }
            | Delivery::Comment { .. }
            | Delivery::Pull { .. }
            | Delivery::Review { .. }
            | Delivery::Remark { .. }
            | Delivery::Push { .. }
            | Delivery::Ref { .. }
            | Delivery::Ci { .. }
            | Delivery::Other => {}
        }
        if let Some(hint) = github::delivery::hint(forge, &self.settings.word, &logged.delivery) {
            if let Some(path) = logged.repository
                && let Some(row) = self.github_tables.repository(hint.repository)
            {
                // A path alone cannot supply a missing row's owner account or settings (4.2).
                if row.owner == path.owner {
                    let row = github::plans::tables::Repository { name: path.name, ..row.clone() };
                    self.github_tables.keep_repository(hint.repository, row);
                } else {
                    // Another owner may be another account, which routes the calls: the next read fills it.
                    self.github_tables.drop_repository(hint.repository);
                }
            }
            let source = *self.own_tokens.get(at).expect("forge token");
            let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
            gh.puller.source = source;
            gh.puller.hold(hint);
        }
    }
}
