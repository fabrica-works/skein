//! GitHub's typed start plans on the forge's reserved connection (forge-connection.md, 3.4).
use super::{Component, OwnPlan, backoff};
use crate::{
    Event, Limits, StartRead, Stopped, Value,
    connection::{self, Exchange, Phase},
    installations::Read,
    start::Start,
};
use skein_forge::{
    api,
    credential::Credential,
    github::{
        self, document,
        plans::{
            Heard,
            plan::{Bounds, Exchange as Pending},
            start::{Identity, InstallationPage, Installations, Minting, Step},
        },
        request::Scope,
    },
};
use skein_lib::{Duration, Env, Queue, Wall};

enum Progress {
    Send(Pending),
    Paged(Result<InstallationPage, api::Error>),
    Minted(Result<document::Minted, api::Error>),
    Identified(Result<document::Viewer, api::Error>),
}
impl Component {
    fn github_bounds(&self, l: &Limits) -> Bounds {
        crate::limits::installation_bounds(l, &self.shared).expect("checked installation bounds")
    }
    pub(super) fn github_wait(&mut self, at: u32, env: &Env<Limits>) {
        let state = self.starts.get_mut(at).expect("forge start");
        let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
        let next = match *state {
            Start::Listing { next } | Start::Minting { next } | Start::Identity { next } => next,
            Start::Ready { .. } => match &gh.read {
                Read::Reading(reading) => reading.next,
                Read::Idle | Read::Changes(_) | Read::Waiting { .. } => return,
            },
            Start::Unconnected
            | Start::Checking { .. }
            | Start::Settings { .. }
            | Start::Waiting { .. }
            | Start::Stopped(_) => return,
        };
        let until = env.now.saturating_add(gh.retry.unwrap_or(Duration::ZERO).max(next));
        let next = next.saturating_mul(2).min(env.limits.start_backoff_max);
        match *state {
            Start::Ready { .. } => gh.read = Read::Waiting { until, next },
            Start::Listing { .. } | Start::Minting { .. } | Start::Identity { .. } => {
                *state = Start::Waiting { until, next };
                gh.read = Read::Idle;
                gh.forget_tokens();
            }
            Start::Unconnected
            | Start::Checking { .. }
            | Start::Settings { .. }
            | Start::Waiting { .. }
            | Start::Stopped(_) => unreachable!("active GitHub work"),
        }
        *self.own_plans.get_mut(at).expect("forge plan") = None;
    }
    fn github_stop(&mut self, at: u32, why: Stopped, up: &mut Queue<Event>) {
        let forge = self.settings.forges.get_mut(at).expect("forge");
        forge.access = None;
        self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state").forget();
        *self.own_plans.get_mut(at).expect("forge plan") = None;
        *self.starts.get_mut(at).expect("forge start") = Start::Stopped(why);
        up.push(Event::Stopped { forge: forge.number, why });
    }
    fn github_select(&mut self, at: u32, next: Duration, up: &mut Queue<Event>) {
        let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
        gh.mint = None;
        gh.stale = false;
        for (id, row) in &gh.rows {
            if row.accepted && !row.suspended {
                let scope = if row.api.is_none() {
                    Some(Scope::Full)
                } else if row.git_expires == Wall::EPOCH {
                    Some(Scope::Contents)
                } else {
                    None
                };
                if let Some(scope) = scope {
                    gh.mint = Some((*id, scope));
                    *self.starts.get_mut(at).expect("forge start") = Start::Minting { next };
                    return;
                }
            }
        }
        // The most recently used accepted Full token is the identity credential.
        let mut recent = None;
        if let Some(id) = gh.recent
            && let Some(row) = gh.rows.get(&id)
            && row.accepted
            && !row.suspended
            && row.api.is_some()
        {
            recent = Some(id);
        }
        if recent.is_none() {
            for (id, row) in &gh.rows {
                if row.accepted && !row.suspended && row.api.is_some() {
                    recent = Some(*id);
                }
            }
        }
        gh.recent = recent;
        if recent.is_some() && !gh.identity_checked {
            *self.starts.get_mut(at).expect("forge start") = Start::Identity { next };
        } else {
            *self.starts.get_mut(at).expect("forge start") = Start::Ready { cap: github::pin::PAGE_CAP };
            if !gh.connected {
                gh.connected = true;
                up.push(Event::Connected { forge: self.settings.forges.get(at).expect("forge").number });
            }
        }
    }
    fn github_progress(&mut self, at: u32, progress: Progress, env: &Env<Limits>, up: &mut Queue<Event>) {
        match progress {
            Progress::Send(exchange) => {
                match self.own_plans.get_mut(at).expect("forge plan").as_mut().expect("GitHub plan") {
                    OwnPlan::GitHubInstallations(plan) => plan.set_pending(exchange),
                    OwnPlan::GitHubMinting(plan) => plan.set_pending(exchange),
                    OwnPlan::GitHubIdentity(plan) => plan.set_pending(exchange),
                    OwnPlan::Forgejo(_) => unreachable!("GitHub plan"),
                }
                return;
            }
            Progress::Paged(answer) => {
                *self.own_plans.get_mut(at).expect("forge plan") = None;
                let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
                if gh.stale_read() {
                    gh.read = Read::Idle;
                    gh.reread = true;
                    return;
                }
                let result = match answer {
                    Ok(page) => {
                        let reading = match &mut gh.read {
                            Read::Reading(reading) => reading,
                            Read::Idle | Read::Changes(_) | Read::Waiting { .. } => unreachable!("active read"),
                        };
                        match reading.page(page.rows, &gh.rows, &gh.accepted, self.shared.name_bytes) {
                            Ok(()) => {
                                if page.next {
                                    match reading.page.checked_add(1) {
                                        Some(next) => {
                                            reading.page = next;
                                            Ok(())
                                        }
                                        None => Err(api::Error::TooLarge),
                                    }
                                } else {
                                    reading.complete = true;
                                    Ok(())
                                }
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    self.github_error(at, error, env, up);
                }
                return;
            }
            Progress::Minted(answer) => {
                if !self.github_minted(at, answer, env, up) {
                    return;
                }
            }
            Progress::Identified(answer) => match answer {
                Ok(viewer) => {
                    if viewer.id != self.settings.forges.get(at).expect("forge").access.as_ref().expect("access").user {
                        self.github_stop(at, Stopped::OtherUser { user: viewer.id }, up);
                        return;
                    }
                    self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state").identity_checked =
                        true;
                }
                Err(error) => {
                    self.github_error(at, error, env, up);
                    return;
                }
            },
        }
        *self.own_plans.get_mut(at).expect("forge plan") = None;
        let next = active_next(*self.starts.get(at).expect("forge start"));
        if self.github.get(at).expect("forge slot").as_ref().expect("GitHub state").reread {
            *self.starts.get_mut(at).expect("forge start") = Start::Listing { next };
        } else {
            self.github_select(at, next, up);
        }
    }
    fn github_minted(
        &mut self,
        at: u32,
        answer: Result<document::Minted, api::Error>,
        env: &Env<Limits>,
        up: &mut Queue<Event>,
    ) -> bool {
        if self.github.get(at).expect("forge slot").as_ref().expect("GitHub state").stale {
            drop(answer);
        } else {
            match answer {
                Ok(minted) => {
                    if !crate::limits::header_value(minted.token.expose())
                        || minted.token.len() > usize::try_from(env.limits.token_bytes).expect("token bound")
                        || minted.expires <= env.wall
                    {
                        self.github_wait(at, env);
                        return false;
                    }
                    let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
                    let (id, scope) = gh.mint.expect("selected mint");
                    let row = gh.rows.get_mut(&id).expect("selected row");
                    if row.accepted && !row.suspended {
                        match scope {
                            Scope::Full => {
                                row.api = Some(minted.token);
                                row.api_expires = minted.expires;
                                gh.recent = Some(id);
                            }
                            Scope::Contents => {
                                row.git_expires = minted.expires;
                                up.push(Event::GitToken {
                                    forge: self.settings.forges.get(at).expect("forge").number,
                                    installation: id,
                                    account: row.account.clone(),
                                    token: minted.token,
                                    expires: minted.expires,
                                });
                            }
                        }
                    }
                }
                Err(error) => {
                    self.github_error(at, error, env, up);
                    return false;
                }
            }
        }
        true
    }
    fn github_error(&mut self, at: u32, _error: api::Error, env: &Env<Limits>, up: &mut Queue<Event>) {
        let gh = self.github.get(at).expect("forge slot").as_ref().expect("GitHub state");
        if let Some(status) = gh.status
            && (400..500).contains(&status)
            && status != 429
        {
            let why = match *self.starts.get(at).expect("forge start") {
                Start::Listing { .. } | Start::Ready { .. } => Some(if status == 401 {
                    Stopped::KeyRefused
                } else {
                    Stopped::Refused { read: StartRead::Installations, status }
                }),
                Start::Minting { .. } => {
                    if status == 401 {
                        Some(Stopped::KeyRefused)
                    } else if status == 403 || status == 404 {
                        None
                    } else {
                        Some(Stopped::TokenRefused { installation: gh.mint.expect("mint").0, status })
                    }
                }
                Start::Identity { .. } => Some(Stopped::Refused { read: StartRead::Viewer, status }),
                Start::Unconnected
                | Start::Checking { .. }
                | Start::Settings { .. }
                | Start::Waiting { .. }
                | Start::Stopped(_) => None,
            };
            if let Some(why) = why {
                self.github_stop(at, why, up);
                return;
            }
        }
        self.github_wait(at, env);
    }
    fn github_begin(&mut self, at: u32, state: Start, env: &Env<Limits>, up: &mut Queue<Event>) {
        let mut plan = match state {
            Start::Listing { .. } | Start::Ready { .. } => {
                let gh = self.github.get(at).expect("forge slot").as_ref().expect("GitHub state");
                let page = match &gh.read {
                    Read::Reading(reading) => reading.page,
                    Read::Idle | Read::Changes(_) | Read::Waiting { .. } => unreachable!("selected read"),
                };
                OwnPlan::GitHubInstallations(Installations::new(page))
            }
            Start::Minting { .. } => {
                let gh = self.github.get(at).expect("forge slot").as_ref().expect("GitHub state");
                let (id, scope) = gh.mint.expect("selected mint");
                OwnPlan::GitHubMinting(Minting::new(id, scope))
            }
            Start::Identity { .. } => OwnPlan::GitHubIdentity(Identity::new()),
            Start::Unconnected
            | Start::Checking { .. }
            | Start::Settings { .. }
            | Start::Waiting { .. }
            | Start::Stopped(_) => unreachable!("active GitHub start"),
        };
        let bounds = self.github_bounds(&env.limits);
        let progress = match &mut plan {
            OwnPlan::GitHubInstallations(plan) => match plan.begin(bounds) {
                Ok(Step::Send(send)) => Progress::Send(send),
                Ok(Step::Done(done)) => Progress::Paged(done),
                Err(error) => Progress::Paged(Err(error)),
            },
            OwnPlan::GitHubMinting(plan) => match plan.begin(bounds) {
                Ok(Step::Send(send)) => Progress::Send(send),
                Ok(Step::Done(done)) => Progress::Minted(done),
                Err(error) => Progress::Minted(Err(error)),
            },
            OwnPlan::GitHubIdentity(plan) => match plan.begin(bounds) {
                Ok(Step::Send(send)) => Progress::Send(send),
                Ok(Step::Done(done)) => Progress::Identified(done),
                Err(error) => Progress::Identified(Err(error)),
            },
            OwnPlan::Forgejo(_) => unreachable!("GitHub plan"),
        };
        *self.own_plans.get_mut(at).expect("forge plan") = Some(plan);
        self.github_progress(at, progress, env, up);
    }
    fn github_send(
        &mut self,
        at: u32,
        id: skein_lib::Id<connection::Connection>,
        env: &Env<Limits>,
        up: &mut Queue<Event>,
    ) -> bool {
        let state = *self.starts.get(at).expect("forge start");
        let c = self.connection_slab.get_mut(id).expect("own connection");
        if c.phase == Phase::Idle {
            let gh = self.github.get(at).expect("forge slot").as_ref().expect("GitHub state");
            let identity_without_token = match state {
                Start::Identity { .. } => gh.recent.is_none(),
                Start::Listing { .. }
                | Start::Minting { .. }
                | Start::Unconnected
                | Start::Checking { .. }
                | Start::Settings { .. }
                | Start::Waiting { .. }
                | Start::Ready { .. }
                | Start::Stopped(_) => false,
            };
            if gh.stale_read() {
                let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
                gh.read = Read::Idle;
                gh.reread = true;
                *self.own_plans.get_mut(at).expect("forge plan") = None;
                return false;
            }
            if gh.stale || identity_without_token {
                *self.own_plans.get_mut(at).expect("forge plan") = None;
                self.github_select(at, active_next(state), up);
                return false;
            }
            let pending = match self.own_plans.get_mut(at).expect("forge plan").as_mut().expect("GitHub plan") {
                OwnPlan::GitHubInstallations(plan) => plan.pending(),
                OwnPlan::GitHubMinting(plan) => plan.pending(),
                OwnPlan::GitHubIdentity(plan) => plan.pending(),
                OwnPlan::Forgejo(_) => unreachable!("GitHub plan"),
            };
            if let Some(exchange) = pending {
                let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
                gh.status = None;
                gh.retry = None;
                let forge = self.settings.forges.get(at).expect("forge");
                let secret = match state {
                    Start::Identity { .. } => {
                        let row = gh.rows.get(&gh.recent.expect("identity row")).expect("identity row");
                        if !row.accepted || row.api.is_none() {
                            *self.own_plans.get_mut(at).expect("forge plan") = None;
                            let next = active_next(state);
                            self.github_select(at, next, up);
                            return false;
                        }
                        row.api.clone()
                    }
                    Start::Listing { .. } | Start::Minting { .. } | Start::Ready { .. } => {
                        match &forge.access.as_ref().expect("access").credential {
                            Credential::App { id, key } => {
                                gh.jwt.bearer(*id, key, env.wall, env.limits.deadline).cloned()
                            }
                            Credential::Token(_) => unreachable!("checked App access"),
                        }
                    }
                    Start::Unconnected
                    | Start::Checking { .. }
                    | Start::Settings { .. }
                    | Start::Waiting { .. }
                    | Start::Stopped(_) => unreachable!("active GitHub start"),
                };
                let Some(secret) = secret else {
                    self.github_stop(at, Stopped::Invalid { value: Value::Key }, up);
                    return false;
                };
                let authorization = github::credential::bearer(&secret).expect("bounded bearer");
                c.deadline = Some(env.now.saturating_add(env.limits.deadline));
                if c.start(
                    env,
                    forge,
                    &self.settings.user_agent,
                    Exchange::GitHub { request: exchange.request, authorization: Some(authorization) },
                )
                .is_err()
                {
                    self.github_wait(at, env);
                    return false;
                }
            }
        }
        true
    }
    fn github_heard(&mut self, at: u32, heard: connection::Heard, env: &Env<Limits>, up: &mut Queue<Event>) {
        let heard = match heard {
            connection::Heard::GitHub(heard) => heard,
            connection::Heard::Forgejo(_) => unreachable!("GitHub connection"),
        };
        let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
        match &heard {
            Heard::Head { status, head } => {
                gh.status = Some(*status);
                gh.retry = head.retry_after;
            }
            Heard::Row(
                document::Row::Installation(_)
                | document::Row::Reachable(_)
                | document::Row::Branch(_)
                | document::Row::Issue(_)
                | document::Row::Pull(_)
                | document::Row::Context(_)
                | document::Row::Run(_)
                | document::Row::Comment(_)
                | document::Row::Collaborator(_)
                | document::Row::Remark(_)
                | document::Row::Review(_)
                | document::Row::Commit(_)
                | document::Row::File(_),
            )
            | Heard::Done(_)
            | Heard::Failed(_) => {}
        }
        let bounds = self.github_bounds(&env.limits);
        let progress = match self.own_plans.get_mut(at).expect("forge plan").as_mut().expect("GitHub plan") {
            OwnPlan::GitHubInstallations(plan) => match plan.heard(heard, bounds) {
                Ok(Some(Step::Send(send))) => Some(Progress::Send(send)),
                Ok(Some(Step::Done(done))) => Some(Progress::Paged(done)),
                Ok(None) => None,
                Err(error) => Some(Progress::Paged(Err(error))),
            },
            OwnPlan::GitHubMinting(plan) => match plan.heard(heard, bounds) {
                Ok(Some(Step::Send(send))) => Some(Progress::Send(send)),
                Ok(Some(Step::Done(done))) => Some(Progress::Minted(done)),
                Ok(None) => None,
                Err(error) => Some(Progress::Minted(Err(error))),
            },
            OwnPlan::GitHubIdentity(plan) => match plan.heard(heard, bounds) {
                Ok(Some(Step::Send(send))) => Some(Progress::Send(send)),
                Ok(Some(Step::Done(done))) => Some(Progress::Identified(done)),
                Ok(None) => None,
                Err(error) => Some(Progress::Identified(Err(error))),
            },
            OwnPlan::Forgejo(_) => unreachable!("GitHub plan"),
        };
        if let Some(progress) = progress {
            self.github_progress(at, progress, env, up);
        }
    }
    // Local output consumes only above room; socket work separately reserves below room.
    fn github_local(&mut self, at: u32, env: &Env<Limits>, up: &mut Queue<Event>) -> bool {
        let state = *self.starts.get(at).expect("forge start");
        let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
        let forge = self.settings.forges.get(at).expect("forge").number;
        match &gh.read {
            Read::Changes(_) => {
                if let Some(event) = gh.output(forge) {
                    up.push(event);
                } else {
                    gh.read = Read::Idle;
                    match state {
                        Start::Listing { next } => self.github_select(at, next, up),
                        Start::Unconnected
                        | Start::Checking { .. }
                        | Start::Settings { .. }
                        | Start::Minting { .. }
                        | Start::Identity { .. }
                        | Start::Waiting { .. }
                        | Start::Ready { .. }
                        | Start::Stopped(_) => {}
                    }
                }
                true
            }
            Read::Reading(reading) => {
                if reading.fact.is_some() {
                    up.push(gh.output(forge).expect("held fact"));
                    true
                } else if reading.complete {
                    if gh.stale_read() {
                        gh.read = Read::Idle;
                        gh.reread = true;
                    } else {
                        gh.commit();
                    }
                    true
                } else {
                    false
                }
            }
            Read::Waiting { until, next } => {
                if *until <= env.now {
                    let next = *next;
                    gh.begin_read(next);
                    true
                } else {
                    false
                }
            }
            Read::Idle => match state {
                Start::Listing { next } => {
                    gh.begin_read(next);
                    true
                }
                Start::Ready { .. } => {
                    if gh.reread {
                        gh.begin_read(env.limits.start_backoff);
                        true
                    } else {
                        false
                    }
                }
                Start::Unconnected
                | Start::Checking { .. }
                | Start::Settings { .. }
                | Start::Minting { .. }
                | Start::Identity { .. }
                | Start::Waiting { .. }
                | Start::Stopped(_) => false,
            },
        }
    }
    pub(super) fn drive_github(
        &mut self,
        at: u32,
        env: &Env<Limits>,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
        budget: (u32, u32),
    ) {
        let above = up.len();
        let below = io.len();
        if let Some((until, next)) = backoff(*self.starts.get(at).expect("forge start"))
            && until <= env.now
            && !self.closed
        {
            *self.starts.get_mut(at).expect("forge start") = Start::Listing { next };
        }
        for turn in 0..64_u32 {
            if up.len().saturating_sub(above) >= budget.0 {
                self.wake = Some(env.now);
                break;
            }
            if !self.closed && self.github_local(at, env, up) {
                if turn == 63 {
                    self.wake = Some(env.now);
                }
                continue;
            }
            if up.len().saturating_sub(above).saturating_add(2) > budget.0
                || io.len().saturating_sub(below).saturating_add(2) > budget.1
            {
                self.wake = Some(env.now);
                break;
            }
            let state = *self.starts.get(at).expect("forge start");
            let gh = self.github.get(at).expect("forge slot").as_ref().expect("GitHub state");
            let reading = match gh.read {
                Read::Reading(_) => true,
                Read::Idle | Read::Changes(_) | Read::Waiting { .. } => false,
            };
            let active = github_active(state) || reading;
            if !self.closed && active && self.own_plans.get(at).expect("forge plan").is_none() {
                self.github_begin(at, state, env, up);
                continue;
            }
            let mut token = *self.own_tokens.get(at).expect("forge token");
            if !self.closed && active && token.is_none() && self.connections.len() < self.connections.capacity() {
                token = Some(self.open_own(at, env, io));
            }
            let Some(token) = token else {
                break;
            };
            let id = *self.connections.get(&token).expect("own connection");
            let c = self.connection_slab.get_mut(id).expect("own connection");
            if self.closed || !active {
                if c.deadline.is_none() && c.phase == Phase::Idle {
                    c.deadline = Some(env.now.saturating_add(env.limits.idle));
                }
                if self.closed || c.phase != Phase::Idle || c.deadline <= Some(env.now) {
                    c.close(c.in_flight(), io);
                }
                break;
            }
            if c.closing {
                break;
            }
            if let Some(deadline) = c.deadline
                && deadline <= env.now
            {
                c.timeout();
            }
            if !self.github_send(at, id, env, up) {
                continue;
            }
            let c = self.connection_slab.get_mut(id).expect("own connection");
            c.pump(env, io, budget.1.saturating_sub(io.len().saturating_sub(below)).saturating_sub(1));
            if let Some(heard) = c.heard() {
                self.github_heard(at, heard, env, up);
                let c = self.connection_slab.get_mut(id).expect("own connection");
                // A plan can finish on the head, leaving an unread exchange behind.
                if self.own_plans.get(at).expect("forge plan").is_none() {
                    if c.in_flight() {
                        c.close(true, io);
                    } else {
                        c.deadline = None;
                    }
                }
                continue;
            }
            if c.phase == Phase::Closed {
                c.close(c.in_flight(), io);
                break;
            }
            if !c.ready() {
                break;
            }
            if turn == 63 {
                self.wake = Some(env.now);
            }
        }
    }
}
fn github_active(state: Start) -> bool {
    match state {
        Start::Listing { .. } | Start::Minting { .. } | Start::Identity { .. } => true,
        Start::Unconnected
        | Start::Checking { .. }
        | Start::Settings { .. }
        | Start::Waiting { .. }
        | Start::Ready { .. }
        | Start::Stopped(_) => false,
    }
}
fn active_next(state: Start) -> Duration {
    match state {
        Start::Listing { next } | Start::Minting { next } | Start::Identity { next } => next,
        Start::Unconnected
        | Start::Checking { .. }
        | Start::Settings { .. }
        | Start::Waiting { .. }
        | Start::Ready { .. }
        | Start::Stopped(_) => unreachable!("active GitHub start"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Account, Change, Fact, MAX_OUT, Request,
        installations::{Metadata, Row},
        tests::fixtures,
    };
    use alloc::boxed::Box;
    use skein_lib::{List, Time};

    // More changes than one entry point reserves, including lifecycle then rename on id 1.
    const ROWS: u32 = MAX_OUT.above.saturating_add(4);
    fn pending() -> (Component, Env<Limits>) {
        let limits = Limits { installations: ROWS, ..fixtures::limits() };
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        let mut component = Component::new(fixtures::github_settings(), &limits, &fixtures::shared()).expect("fixture");
        let gh = component.github.get_mut(0).expect("forge").as_mut().expect("GitHub");
        gh.baseline = false;
        drop(
            gh.rows
                .insert(
                    1,
                    Row {
                        account: Account { id: 22, login: Box::from(b"acme".as_slice()) },
                        suspended: false,
                        accepted: false,
                        api: None,
                        api_expires: Wall::EPOCH,
                        git_expires: Wall::EPOCH,
                    },
                )
                .expect("old row"),
        );
        gh.begin_read(limits.start_backoff);
        match &mut gh.read {
            Read::Reading(reading) => {
                let _seen = reading.seen.insert(1).expect("seen");
                for id in 1..=ROWS {
                    drop(
                        reading
                            .staged
                            .insert(
                                u64::from(id),
                                Metadata {
                                    account: Account {
                                        id: u64::from(id).saturating_add(21),
                                        login: Box::from(b"renamed".as_slice()),
                                    },
                                    suspended: true,
                                    seen: true,
                                },
                            )
                            .expect("staged rows"),
                    );
                }
            }
            Read::Idle | Read::Changes(_) | Read::Waiting { .. } => unreachable!("reading"),
        }
        gh.commit();
        (component, env)
    }
    fn installation(id: u64, login: &[u8]) -> document::Installation {
        document::Installation {
            id,
            account: 22,
            login: Box::from(login),
            kind: document::Account::Organization,
            created: Wall::EPOCH,
            suspended: false,
        }
    }
    fn queues() -> (Queue<Event>, Queue<skein_io::Request>) {
        (Queue::with_capacity(128), Queue::with_capacity(128))
    }
    #[test]
    fn installation_output_resumes_with_remaining_room() {
        let (mut component, env) = pending();
        let (mut up, mut io) = queues();
        // The caller has spent all below room and all but one above slot on other work.
        component.drive_room(&env, &mut up, &mut io, 1, 0);
        match up.pop().expect("first phase") {
            Event::Installation { installation, change, .. } => {
                assert_eq!((installation, change), (1, Change::Suspended));
            }
            Event::Fact { .. }
            | Event::GitToken { .. }
            | Event::Answered(_)
            | Event::Hint(_)
            | Event::ListenerLost
            | Event::Connected { .. }
            | Event::Stopped { .. }
            | Event::Closed => unreachable!("lifecycle"),
        }
        assert_eq!(component.next_deadline(), Some(env.now));
        let mut changes = List::with_capacity(ROWS.saturating_add(1));
        for _ in 0_u32..4 {
            component.fire(&env, &mut up, &mut io);
            assert!(up.len() <= MAX_OUT.above);
            for _ in 0..up.len() {
                match up.pop().expect("queued") {
                    Event::Installation { installation, change, .. } => {
                        changes.push((installation, change)).expect("bounded changes");
                    }
                    Event::Connected { .. } => assert_eq!(changes.len(), ROWS),
                    Event::Fact { .. }
                    | Event::GitToken { .. }
                    | Event::Answered(_)
                    | Event::Hint(_)
                    | Event::ListenerLost
                    | Event::Stopped { .. }
                    | Event::Closed => unreachable!("change output"),
                }
            }
        }
        assert_eq!(changes.len(), ROWS);
        assert_eq!(changes.get(0), Some(&(1, Change::Renamed)));
        for (index, (id, change)) in changes.as_slice().iter().enumerate().skip(1) {
            assert_eq!((*id, *change), (u64::try_from(index).expect("index").saturating_add(1), Change::Created));
        }
        assert!(io.is_empty());
        assert_eq!(component.next_deadline(), None);
    }
    #[test]
    fn held_unkept_blocks_the_next_page_until_output_room() {
        let limits = Limits { installations: 1, ..fixtures::limits() };
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        let mut component = Component::new(fixtures::github_settings(), &limits, &fixtures::shared()).expect("fixture");
        let gh = component.github.get_mut(0).expect("forge").as_mut().expect("GitHub");
        gh.begin_read(limits.start_backoff);
        let mut rows = List::with_capacity(2);
        for id in [1, 2] {
            rows.push(installation(id, b"name")).expect("page");
        }
        match &mut gh.read {
            Read::Reading(reading) => {
                reading
                    .page(rows.into_boxed(), &gh.rows, &gh.accepted, fixtures::shared().name_bytes)
                    .expect("bounded page");
                reading.page = 2;
            }
            Read::Idle | Read::Changes(_) | Read::Waiting { .. } => unreachable!("reading"),
        }
        let (mut up, mut io) = queues();
        component.drive_room(&env, &mut up, &mut io, 0, MAX_OUT.below);
        assert!(io.is_empty());
        assert!(up.is_empty());
        component.drive_room(&env, &mut up, &mut io, 1, 0);
        match up.pop().expect("held fact") {
            Event::Fact { fact, .. } => assert_eq!(fact, Fact::Unkept { installation: 2 }),
            Event::Installation { .. }
            | Event::GitToken { .. }
            | Event::Answered(_)
            | Event::Hint(_)
            | Event::ListenerLost
            | Event::Connected { .. }
            | Event::Stopped { .. }
            | Event::Closed => unreachable!("fact"),
        }
        assert!(io.is_empty());
        component.fire(&env, &mut up, &mut io);
        assert_eq!(io.len(), 1);
        assert!(up.is_empty());
    }
    #[test]
    fn close_abandons_pending_installation_output() {
        let (mut component, env) = pending();
        let (mut up, mut io) = queues();
        component.drive_room(&env, &mut up, &mut io, 1, 0);
        drop(up.pop());
        component.down(&env, Request::Close, &mut up, &mut io);
        match up.pop().expect("closed") {
            Event::Closed => {}
            Event::Installation { .. }
            | Event::Fact { .. }
            | Event::GitToken { .. }
            | Event::Answered(_)
            | Event::Hint(_)
            | Event::ListenerLost
            | Event::Connected { .. }
            | Event::Stopped { .. } => unreachable!("Closed last"),
        }
        component.fire(&env, &mut up, &mut io);
        assert!(up.is_empty());
        assert!(io.is_empty());
    }
    #[test]
    fn connect_replacement_abandons_pending_installation_output() {
        let (mut component, env) = pending();
        let (mut up, mut io) = queues();
        component.drive_room(&env, &mut up, &mut io, 1, 0);
        drop(up.pop());
        component.down(
            &env,
            Request::Connect {
                forge: fixtures::FORGE,
                access: fixtures::app_access(skein_fake_forge_github::keys::KEYS.first().expect("public key"), 7, 9),
            },
            &mut up,
            &mut io,
        );
        assert!(up.is_empty());
        assert_eq!(io.len(), 1);
        let gh = component.github.get(0).expect("forge").as_ref().expect("GitHub");
        match gh.read {
            Read::Reading(_) => {}
            Read::Idle | Read::Changes(_) | Read::Waiting { .. } => unreachable!("replacement read"),
        }
    }
    #[test]
    fn installation_page_number_overflow_fails_the_attempt() {
        let limits = fixtures::limits();
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        let mut component = Component::new(fixtures::github_settings(), &limits, &fixtures::shared()).expect("fixture");
        let gh = component.github.get_mut(0).expect("forge").as_mut().expect("GitHub");
        gh.begin_read(limits.start_backoff);
        match &mut gh.read {
            Read::Reading(reading) => reading.page = u32::MAX,
            Read::Idle | Read::Changes(_) | Read::Waiting { .. } => unreachable!("reading"),
        }
        let (mut up, mut io) = queues();
        component.github_progress(
            0,
            Progress::Paged(Ok(InstallationPage { rows: Box::from([]), next: true })),
            &env,
            &mut up,
        );
        assert_eq!(
            component.start(fixtures::FORGE),
            Some(Start::Waiting {
                until: env.now.saturating_add(limits.start_backoff),
                next: limits.start_backoff.saturating_mul(2)
            })
        );
        assert!(up.is_empty());
        component.fire(&env, &mut up, &mut io);
        assert!(io.is_empty());
    }

    #[test]
    fn failed_later_page_preserves_committed_table() {
        let limits = Limits { installations: 1, ..fixtures::limits() };
        let mut env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        let mut component = Component::new(fixtures::github_settings(), &limits, &fixtures::shared()).expect("fixture");
        *component.starts.get_mut(0).expect("start") = Start::Ready { cap: 100 };
        let gh = component.github.get_mut(0).expect("forge").as_mut().expect("GitHub");
        gh.baseline = false;
        gh.connected = true;
        gh.accept(&[1]);
        let token = b"artificial-api";
        let expiry = Wall::from_nanos(3_600_000_000_000);
        drop(
            gh.rows
                .insert(
                    1,
                    Row {
                        account: Account { id: 22, login: Box::from(b"acme".as_slice()) },
                        suspended: false,
                        accepted: true,
                        api: Some(skein_lib::Secret::new(Box::from(token.as_slice()))),
                        api_expires: expiry,
                        git_expires: expiry,
                    },
                )
                .expect("committed row"),
        );
        gh.begin_read(limits.start_backoff);
        let (mut up, mut io) = queues();
        let mut staged = List::with_capacity(2);
        for (id, login) in [(1, b"renamed".as_slice()), (2, b"stray".as_slice())] {
            staged.push(installation(id, login)).expect("page rows");
        }
        component.github_progress(
            0,
            Progress::Paged(Ok(InstallationPage { rows: staged.into_boxed(), next: true })),
            &env,
            &mut up,
        );
        component.drive_room(&env, &mut up, &mut io, 1, 0);
        component.github_progress(0, Progress::Paged(Err(api::Error::InvalidAnswer)), &env, &mut up);
        let gh = component.github.get(0).expect("forge").as_ref().expect("GitHub");
        let row = gh.rows.get(&1).expect("committed row");
        assert_eq!(
            (row.account.login.as_ref(), row.api_expires, row.git_expires),
            (b"acme".as_slice(), expiry, expiry)
        );
        assert!(row.api.as_ref().expect("API token").expose() == token);
        assert_eq!(gh.rows.len(), 1);
        assert_eq!(up.len(), 1);
        match up.pop().expect("capacity diagnostic survives failure") {
            Event::Fact { fact, .. } => assert_eq!(fact, Fact::Unkept { installation: 2 }),
            Event::Installation { .. }
            | Event::GitToken { .. }
            | Event::Answered(_)
            | Event::Hint(_)
            | Event::ListenerLost
            | Event::Connected { .. }
            | Event::Stopped { .. }
            | Event::Closed => unreachable!("one fact"),
        }
        env.now = env.now.saturating_add(limits.start_backoff);
        let _progressed = component.github_local(0, &env, &mut up);
        let page = installation(1, b"acme");
        component.github_progress(
            0,
            Progress::Paged(Ok(InstallationPage { rows: Box::from([page]), next: false })),
            &env,
            &mut up,
        );
        component.drive_room(&env, &mut up, &mut io, MAX_OUT.above, MAX_OUT.below);
        assert!(up.is_empty());
        assert!(io.is_empty());
        assert!(
            component
                .github
                .get(0)
                .expect("forge")
                .as_ref()
                .expect("GitHub")
                .rows
                .get(&1)
                .expect("row")
                .api
                .as_ref()
                .expect("preserved")
                .expose()
                == token
        );
    }
}
