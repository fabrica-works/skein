//! GitHub's typed start plans on the forge's reserved connection (forge-connection.md, 3.4).
use super::{Component, OwnPlan, backoff};
use crate::{
    Account, Event, Limits, StartRead, Stopped, Value,
    connection::{self, Exchange, Phase},
    installations::Row,
    start::Start,
};
use alloc::boxed::Box;
use skein_forge::{
    api,
    credential::Credential,
    github::{
        self, document,
        plans::{
            Heard,
            plan::{Bounds, Exchange as Pending},
            start::{Identity, Listing, Minting, Step},
        },
        request::Scope,
    },
};
use skein_lib::{Duration, Env, Map, Queue, Wall};

enum Progress {
    Send(Pending),
    Listed(Result<Box<[document::Installation]>, api::Error>),
    Minted(Result<document::Minted, api::Error>),
    Identified(Result<document::Viewer, api::Error>),
}
impl Component {
    fn github_bounds(&self, l: &Limits) -> Bounds {
        // Listing measures one boxed string per row; room also covers each retained installation.
        let width = u32::try_from(size_of::<document::Installation>())
            .expect("row fits")
            .checked_add(self.shared.name_bytes)
            .expect("checked listing room");
        Bounds {
            page_size: github::pin::PAGE_CAP,
            rows: l.installations,
            answer_bytes: l.installations.checked_mul(width).expect("checked listing room"),
            body_bytes: l.body_bytes,
        }
    }
    pub(super) fn github_wait(&mut self, at: u32, env: &Env<Limits>) {
        let state = self.starts.get_mut(at).expect("forge start");
        let next = match *state {
            Start::Listing { next } | Start::Minting { next } | Start::Identity { next } => next,
            Start::Unconnected
            | Start::Checking { .. }
            | Start::Settings { .. }
            | Start::Waiting { .. }
            | Start::Ready { .. }
            | Start::Stopped(_) => return,
        };
        let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
        let wait = gh.retry.unwrap_or(Duration::ZERO).max(next);
        *state = Start::Waiting {
            until: env.now.saturating_add(wait),
            next: next.saturating_mul(2).min(env.limits.start_backoff_max),
        };
        gh.forget();
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
            up.push(Event::Connected { forge: self.settings.forges.get(at).expect("forge").number });
        }
    }
    fn github_progress(&mut self, at: u32, progress: Progress, env: &Env<Limits>, up: &mut Queue<Event>) {
        match progress {
            Progress::Send(exchange) => {
                match self.own_plans.get_mut(at).expect("forge plan").as_mut().expect("GitHub plan") {
                    OwnPlan::GitHubListing(plan) => plan.set_pending(exchange),
                    OwnPlan::GitHubMinting(plan) => plan.set_pending(exchange),
                    OwnPlan::GitHubIdentity(plan) => plan.set_pending(exchange),
                    OwnPlan::Forgejo(_) => unreachable!("GitHub plan"),
                }
                return;
            }
            Progress::Listed(answer) => match answer {
                Ok(rows) => {
                    let gh = self.github.get_mut(at).expect("forge slot").as_mut().expect("GitHub state");
                    let mut table = Map::with_capacity(env.limits.installations);
                    for row in rows {
                        if row.login.len() > usize::try_from(self.shared.name_bytes).expect("name bound") {
                            self.github_wait(at, env);
                            return;
                        }
                        drop(
                            table
                                .insert(
                                    row.id,
                                    Row {
                                        account: Account { id: row.account, login: row.login },
                                        suspended: row.suspended,
                                        accepted: gh.accepted.contains(&row.id),
                                        api: None,
                                        api_expires: Wall::EPOCH,
                                        git_expires: Wall::EPOCH,
                                    },
                                )
                                .expect("bounded listing"),
                        );
                    }
                    gh.rows = table;
                }
                Err(error) => {
                    self.github_error(at, error, env, up);
                    return;
                }
            },
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
        self.github_select(at, next, up);
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
                Start::Listing { .. } => Some(if status == 401 {
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
                | Start::Ready { .. }
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
            Start::Listing { .. } => OwnPlan::GitHubListing(Listing::new()),
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
            | Start::Ready { .. }
            | Start::Stopped(_) => unreachable!("active GitHub start"),
        };
        let bounds = self.github_bounds(&env.limits);
        let progress = match &mut plan {
            OwnPlan::GitHubListing(plan) => match plan.begin(bounds) {
                Ok(Step::Send(send)) => Progress::Send(send),
                Ok(Step::Done(done)) => Progress::Listed(done),
                Err(error) => Progress::Listed(Err(error)),
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
            if gh.stale || identity_without_token {
                *self.own_plans.get_mut(at).expect("forge plan") = None;
                self.github_select(at, active_next(state), up);
                return false;
            }
            let pending = match self.own_plans.get_mut(at).expect("forge plan").as_mut().expect("GitHub plan") {
                OwnPlan::GitHubListing(plan) => plan.pending(),
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
                    Start::Listing { .. } | Start::Minting { .. } => {
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
                    | Start::Ready { .. }
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
            Heard::Row(document::Row::Installation(row)) => {
                if row.login.len() > usize::try_from(self.shared.name_bytes).expect("name bound") {
                    self.github_wait(at, env);
                    return;
                }
            }
            Heard::Row(
                document::Row::Reachable(_)
                | document::Row::Branch(_)
                | document::Row::Issue(_)
                | document::Row::Pull(_)
                | document::Row::Context(_)
                | document::Row::Run(_)
                | document::Row::Comment(_),
            )
            | Heard::Done(_)
            | Heard::Failed(_) => {}
        }
        let bounds = self.github_bounds(&env.limits);
        let progress = match self.own_plans.get_mut(at).expect("forge plan").as_mut().expect("GitHub plan") {
            OwnPlan::GitHubListing(plan) => match plan.heard(heard, bounds) {
                Ok(Some(Step::Send(send))) => Some(Progress::Send(send)),
                Ok(Some(Step::Done(done))) => Some(Progress::Listed(done)),
                Ok(None) => None,
                Err(error) => Some(Progress::Listed(Err(error))),
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
        for _ in 0..64_u32 {
            if up.len().saturating_sub(above).saturating_add(2) > budget.0
                || io.len().saturating_sub(below).saturating_add(2) > budget.1
            {
                self.wake = Some(env.now);
                break;
            }
            let state = *self.starts.get(at).expect("forge start");
            let active = github_active(state);
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
                if self.closed || c.phase != Phase::Idle || c.deadline <= Some(env.now) {
                    c.close(c.in_flight(), io);
                }
                break;
            }
            if c.closing {
                break;
            }
            if c.deadline <= Some(env.now) {
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
                if c.in_flight() && self.own_plans.get(at).expect("forge plan").is_none() {
                    c.close(true, io);
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
            self.wake = Some(env.now);
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
