//! The in-process GitHub layer (fake-forge.md, section 10).
use crate::{
    answers::{self, Body, Page},
    app::{self, App},
    installations::{self, Grant, Installations, Kind, Mint, Scope},
    manifest::Manifest,
    pages::{self, Answer, Page as BrowserPage},
    reads, routes,
};
use alloc::boxed::Box;
use skein_fake_forge_domain as fake;
use skein_forge::github::{self, document as d, request, server};
use skein_lib::{List, Map, ReplyTo, Wall, Writer};

/// GitHub's repository metadata absent from the shared domain.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    /// The domain's nonzero repository id.
    pub id: u32,
    /// Owner login.
    pub owner: Box<[u8]>,
    /// Owner's numeric id.
    pub owner_id: u64,
    /// Whether the owner is an organization.
    pub organization: bool,
    /// Repository name without the owner.
    pub name: Box<[u8]>,
    /// Whether the repository is private.
    pub private: bool,
}
/// A scenario account that can own or install the App.
#[derive(Debug)]
pub struct Account {
    pub login: Box<[u8]>,
    pub id: u64,
    pub organization: bool,
}
/// How a scenario presents the converted App's permissions and subscriptions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppFacts {
    /// Keep every permission and event the manifest posts.
    Complete,
    /// Keep the grants, but omit both fields from GET /app.
    Omitted,
    /// Keep only the first permission at read access, and the first event.
    Narrow,
}
/// The scenario's GitHub-only tables, including the signed-in browser account.
#[derive(Debug)]
pub struct Config {
    /// App identity and test signing key.
    pub app: App,
    /// Grants at conversion and fields shown by GET /app.
    pub app_facts: AppFacts,
    /// Domain user acting for every installation.
    pub bot: u64,
    /// Initial webhook URL.
    pub hook: Box<[u8]>,
    /// Repository rows, in any order; ids and owner/name pairs must be unique.
    pub repositories: List<Row>,
    /// Scenario accounts, unique by both login and id, bounded by `github.inner`.
    pub accounts: List<Account>,
    /// Login of the signed-in browser user, from accounts.
    pub person: Box<[u8]>,
}
/// All retained tables and transient input/output are bounded here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Limits {
    /// The dialect's document and target bounds; inner caps repository rows.
    pub github: github::limits::Limits,
    /// Installation, token and per-installation reach capacities.
    pub installations: installations::Limits,
    /// Manifest records and the bytes admitted per manifest.
    pub pages: pages::Limits,
    /// Effective page cap, from one through GitHub's `PAGE_CAP`.
    pub page: u32,
    /// Maximum request body, no larger than `github.body_bytes`.
    pub body: u32,
    /// Maximum response body and, separately, Link value (at least 128 bytes).
    pub response: u32,
}
/// An unusable limit or an invalid scenario table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Problem {
    Limits,
    Config,
}

/// One request above HTTP parsing. Debug never exposes the credential or body.
pub struct Request<'a> {
    /// HTTP method.
    pub method: request::Method,
    /// Canonical dialect target, including paging where required.
    pub target: &'a [u8],
    /// Whole Authorization value; only Bearer is accepted.
    pub authorization: Option<&'a [u8]>,
    /// X-GitHub-Api-Version value.
    pub version: Option<&'a [u8]>,
    /// Whether a User-Agent was supplied.
    pub agent: bool,
    /// Whole bounded request body.
    pub body: &'a [u8],
}
impl core::fmt::Debug for Request<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("target_bytes", &self.target.len())
            .field("authorized", &self.authorization.is_some())
            .field("version", &self.version)
            .field("agent", &self.agent)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}
/// An encoded document, with no HTTP framing. Debug redacts minted tokens.
pub struct Response {
    /// GitHub status.
    pub status: u16,
    /// Optional absolute next and last links.
    pub link: Option<Box<[u8]>>,
    /// JSON document, raw file bytes, or empty on 204.
    pub body: Box<[u8]>,
}
impl core::fmt::Debug for Response {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("link", &self.link)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}
/// An immediate reply or exactly one shared-domain read.
#[derive(Debug)]
pub enum Outcome {
    /// No domain call was needed.
    Reply(Response),
    /// Caller owns the continuation and routes the event's reply to finish.
    Call(Pending, fake::Event),
}
/// One caller-owned continuation; it contains no credential or reply right.
/// The supplied `ReplyTo` moves only into `Event::Call`. The caller bounds its
/// in-flight calls and owns their pending values, as it owns the domain replies.
#[derive(Debug)]
pub struct Pending {
    pub(crate) kind: reads::Pending,
}
/// GitHub's own state, with no clock, socket or HTTP parser.
#[derive(Debug)]
pub struct Service {
    pub(crate) app: App,
    pub(crate) app_facts: AppFacts,
    pub(crate) bot: u64,
    pub(crate) hook: Box<[u8]>,
    pub(crate) accounts: List<Account>,
    pub(crate) person: Box<[u8]>,
    pub(crate) pages: pages::Store,
    pub(crate) repositories: Map<u32, Row>,
    pub(crate) installations: Installations,
    pub(crate) limits: Limits,
}
impl Service {
    /// Validates the tables and limits before creating bounded stores.
    pub fn new(config: Config, limits: &Limits, seed: u64) -> Result<Self, Problem> {
        if worst_case(limits, &config).is_none() {
            return Err(Problem::Limits);
        }
        let string = usize::try_from(limits.github.string_bytes).or(Err(Problem::Limits))?;
        if config.bot == 0
            || config.bot == config.app.id
            || config.app.id == 0
            || config.app.slug.is_empty()
            || config.app.slug.len().checked_add(b"[bot]".len()).ok_or(Problem::Config)? > string
            || config.hook.len() > string
            || config.repositories.len() > limits.github.inner
            || config.app.permissions.len() > usize::try_from(limits.github.inner).or(Err(Problem::Limits))?
            || config.app.events.len() > usize::try_from(limits.github.inner).or(Err(Problem::Limits))?
        {
            return Err(Problem::Config);
        }
        for permission in &config.app.permissions {
            if permission.name.len() > string {
                return Err(Problem::Config);
            }
        }
        for event in &config.app.events {
            if event.len() > string {
                return Err(Problem::Config);
            }
        }
        for (at, row) in config.repositories.iter().enumerate() {
            if row.id == 0
                || row.owner_id == 0
                || row.owner.is_empty()
                || row.name.is_empty()
                || row.owner.len() > string
                || row.name.len() > string
                || row.owner.contains(&b'/')
                || row.name.contains(&b'/')
                || b"https://github.com/"
                    .len()
                    .checked_add(row.owner.len())
                    .ok_or(Problem::Config)?
                    .checked_add(1)
                    .ok_or(Problem::Config)?
                    .checked_add(row.name.len())
                    .ok_or(Problem::Config)?
                    > string
            {
                return Err(Problem::Config);
            }
            for prior in config.repositories.iter().take(at) {
                if prior.id == row.id || (prior.owner == row.owner && prior.name == row.name) {
                    return Err(Problem::Config);
                }
            }
        }
        if config.accounts.len() > limits.github.inner || config.person.len() > string {
            return Err(Problem::Config);
        }
        let mut person_found = false;
        for (at, account) in config.accounts.iter().enumerate() {
            if account.id == 0 || account.login.len() > string || !pages::unreserved(&account.login) {
                return Err(Problem::Config);
            }
            if account.login == config.person {
                person_found = true;
            }
            for prior in config.accounts.iter().take(at) {
                if prior.id == account.id || prior.login == account.login {
                    return Err(Problem::Config);
                }
            }
        }
        if !person_found {
            return Err(Problem::Config);
        }
        let mut repositories = Map::with_capacity(limits.github.inner);
        for row in config.repositories.into_boxed() {
            if repositories.insert(row.id, row).is_err() {
                return Err(Problem::Config);
            }
        }
        Ok(Self {
            app: config.app,
            app_facts: config.app_facts,
            bot: config.bot,
            hook: config.hook,
            accounts: config.accounts,
            person: config.person,
            pages: pages::Store::new(limits.pages, seed),
            repositories,
            installations: Installations::new(&limits.installations, seed),
            limits: *limits,
        })
    }
    /// Scenario access to the bounded store, without touching the domain.
    pub fn installations_mut(&mut self) -> &mut Installations {
        &mut self.installations
    }
    /// Installation state, in id order.
    #[must_use]
    pub const fn installations(&self) -> &Installations {
        &self.installations
    }

    /// Serves browser pages without API headers or a credential, and never calls the domain.
    pub fn page(&mut self, page: &BrowserPage<'_>, now: Wall) -> Answer {
        pages::serve(self, page, now)
    }
    /// The posted manifest stays readable after conversion until its record is reclaimed.
    #[must_use]
    pub fn manifest(&self, state: &[u8]) -> Option<&Manifest> {
        match self.pages.states.get(state) {
            Some(record) => Some(&record.manifest),
            None => None,
        }
    }

    /// Checks User-Agent, version, route, credential, installation state/reach,
    /// then the body. Every refusal precedes a domain call.
    pub fn request(&mut self, request: &Request<'_>, now: Wall, reply_to: ReplyTo) -> Outcome {
        let limits = &self.limits;
        if !request.agent {
            return Outcome::Reply(answers::refusal(403, b"User agent required", limits));
        }
        if request.version != Some(github::pin::API_VERSION.as_bytes()) {
            return Outcome::Reply(answers::refusal(400, b"Invalid API version", limits));
        }
        let Some(route) = routes::decode(request.method, request.target, &limits.github) else {
            return Outcome::Reply(answers::missing(limits));
        };
        match &route {
            routes::Route::BotUser(slug) => {
                return Outcome::Reply(if slug == &self.app.slug {
                    answers::response(200, &Body::BotUser(self.bot, slug), limits)
                } else {
                    answers::missing(limits)
                });
            }
            routes::Route::Conversion(code) => {
                return Outcome::Reply(pages::convert(self, code.expose(), request.body, now));
            }
            routes::Route::App(_) | routes::Route::Token(_) => {}
        }
        let bearer = match request.authorization {
            Some(header) => header.strip_prefix(b"Bearer "),
            None => None,
        };
        let Some(bearer) = bearer else {
            return Outcome::Reply(answers::unauthorized(limits));
        };
        match route {
            routes::Route::Conversion(_) | routes::Route::BotUser(_) => Outcome::Reply(answers::failed()),
            routes::Route::App(route) => {
                if app::check(bearer, &self.app, now).is_err() {
                    return Outcome::Reply(answers::unauthorized(limits));
                }
                Outcome::Reply(self.app_route(route, request, now))
            }
            routes::Route::Token(route) => {
                let Ok(grant) = self.installations.token(bearer, now) else {
                    return Outcome::Reply(answers::unauthorized(limits));
                };
                let grant = *grant;
                let Some(installation) = self.installations.get(grant.installation) else {
                    return Outcome::Reply(answers::unauthorized(limits));
                };
                // Suspension does not revoke the token; resuming the installation restores its use.
                if installation.suspended {
                    return Outcome::Reply(answers::refusal(403, answers::SUSPENDED, limits));
                }
                match route {
                    routes::Token::Reachable(page) => {
                        let response = if self.body_fits(request.body) {
                            self.reachable(page, &grant, request.target)
                        } else {
                            answers::refusal(413, b"Payload Too Large", limits)
                        };
                        Outcome::Reply(response)
                    }
                    routes::Token::User(id) => Outcome::Reply(reads::user(self, id, self.body_fits(request.body))),
                    routes::Token::Graphql => {
                        if !self.body_fits(request.body) {
                            return Outcome::Reply(answers::response(200, &Body::Graphql, limits));
                        }
                        match server::graphql_body(request.body, &limits.github) {
                            Some(server::Graphql::Viewer) => Outcome::Reply(self.viewer()),
                            Some(server::Graphql::Pull { owner, name, number }) => reads::call(
                                self,
                                reads::Read::PullGraphql { owner, name, number },
                                &grant,
                                request.target,
                                self.body_fits(request.body),
                                reply_to,
                            ),
                            None => Outcome::Reply(answers::response(200, &Body::Graphql, limits)),
                        }
                    }
                    routes::Token::QueueMerge { owner, name, number } => {
                        let route = request::Request::Merge {
                            owner, name, number, head: [0; 32], method: github::plans::tables::Method::Merge,
                        };
                        let Some(request::Request::Merge { owner, name, number, head, .. }) =
                            server::write_body(route, request.body, &limits.github)
                        else { return Outcome::Reply(answers::refusal(422, b"Validation Failed", limits)); };
                        reads::call(self, reads::Read::QueueMerge { owner, name, number, head }, &grant,
                            request.target, self.body_fits(request.body), reply_to)
                    }
                    routes::Token::Read(read) => {
                        reads::call(self, read, &grant, request.target, self.body_fits(request.body), reply_to)
                    }
                }
            }
        }
    }
    /// Completes the one domain call. No served route needs a second call.
    pub fn finish(&mut self, pending: Pending, result: Result<fake::api::Answer, fake::api::Error>) -> Response {
        reads::finish(pending, result, &self.limits)
    }
    fn app_route(&mut self, route: routes::App, request: &Request<'_>, now: Wall) -> Response {
        if !self.body_fits(request.body) {
            return match route {
                routes::App::Mint(id) => self.mint(id, request.body, now),
                routes::App::SetHook => self.set_hook(request.body),
                routes::App::Installation(id) | routes::App::Uninstall(id) => {
                    if self.installations.get(id).is_none() {
                        answers::missing(&self.limits)
                    } else {
                        answers::refusal(413, b"Payload Too Large", &self.limits)
                    }
                }
                routes::App::Identity | routes::App::Hook | routes::App::Installations(_) => {
                    answers::refusal(413, b"Payload Too Large", &self.limits)
                }
            };
        }
        match route {
            routes::App::Identity => self.app_identity(),
            routes::App::Hook => answers::response(200, &Body::Hook(&d::Hook { url: self.hook.clone() }), &self.limits),
            routes::App::SetHook => self.set_hook(request.body),
            routes::App::Installations(page) => self.installation_page(page, request.target),
            routes::App::Installation(id) => match self.installations.get(id) {
                Some(row) => answers::response(200, &Body::Installation(&installation(row)), &self.limits),
                None => answers::missing(&self.limits),
            },
            routes::App::Mint(id) => self.mint(id, request.body, now),
            routes::App::Uninstall(id) => {
                if self.installations.remove(id) {
                    Response { status: 204, link: None, body: Box::from([]) }
                } else {
                    answers::missing(&self.limits)
                }
            }
        }
    }
    fn app_identity(&self) -> Response {
        let app = d::App {
            id: self.app.id,
            slug: self.app.slug.clone(),
            permissions: self.app.permissions.clone(),
            events: self.app.events.clone(),
        };
        let body = match self.app_facts {
            AppFacts::Complete | AppFacts::Narrow => Body::App(&app),
            AppFacts::Omitted => Body::AppIdentity(&app),
        };
        answers::response(200, &body, &self.limits)
    }
    fn mint(&mut self, id: u64, body: &[u8], now: Wall) -> Response {
        let Some(row) = self.installations.get(id) else {
            return answers::missing(&self.limits);
        };
        if row.suspended {
            return answers::refusal(403, answers::SUSPENDED, &self.limits);
        }
        let Some(scope) = server::mint_scope(body) else {
            return answers::refusal(422, b"Validation Failed", &self.limits);
        };
        if !self.body_fits(body) {
            return answers::refusal(422, b"Validation Failed", &self.limits);
        }
        let scope = match scope {
            request::Scope::Full => Scope::Full,
            request::Scope::Contents => Scope::Contents,
        };
        match self.installations.mint(id, scope, now) {
            Ok((token, expires)) => answers::response(201, &Body::Minted(&d::Minted { token, expires }), &self.limits),
            Err(Mint::Missing) => answers::missing(&self.limits),
            Err(Mint::Suspended) => answers::refusal(403, answers::SUSPENDED, &self.limits),
            Err(Mint::Full) => answers::refusal(503, b"Server Error", &self.limits),
        }
    }
    fn body_fits(&self, body: &[u8]) -> bool {
        match u32::try_from(body.len()) {
            Ok(length) => length <= self.limits.body,
            Err(_) => false,
        }
    }
    fn set_hook(&mut self, body: &[u8]) -> Response {
        if !self.body_fits(body) {
            return answers::refusal(422, b"Validation Failed", &self.limits);
        }
        let Some(url) = server::hook_url(body, &self.limits.github) else {
            return answers::refusal(422, b"Validation Failed", &self.limits);
        };
        // The hook reader also represents an absent URL as empty; an update
        // needs a usable value and must not erase the current address.
        if url.is_empty() {
            return answers::refusal(422, b"Validation Failed", &self.limits);
        }
        let response = answers::response(200, &Body::Hook(&d::Hook { url: url.clone() }), &self.limits);
        if response.status == 200 {
            self.hook = url;
        }
        response
    }
    fn viewer(&self) -> Response {
        let Some(login) = joined(&self.app.slug, b"[bot]") else {
            return answers::failed();
        };
        answers::response(200, &Body::Viewer(&d::Viewer { id: self.bot, login }), &self.limits)
    }
    fn installation_page(&self, page: u32, target: &[u8]) -> Response {
        let Ok(total) = u64::try_from(self.installations.iter().count()) else {
            return answers::failed();
        };
        let Some(page) = Page::new(page, self.limits.page, total) else {
            return answers::failed();
        };
        let mut rows = List::with_capacity(self.limits.page);
        for (index, row) in self.installations.iter().enumerate() {
            let Ok(index) = u64::try_from(index) else {
                return answers::failed();
            };
            if page.contains(index) && rows.push(installation(row)).is_err() {
                return answers::failed();
            }
        }
        page.link(answers::response(200, &Body::Installations(rows.as_slice()), &self.limits), target, &self.limits)
    }
    // The fake pages by its own limits.page, independent of the request's size.
    fn reachable(&self, number: u32, grant: &Grant, target: &[u8]) -> Response {
        let mut total = 0_u64;
        for (id, _) in &self.repositories {
            if self.installations.reaches(grant, *id) {
                let Some(next) = total.checked_add(1) else {
                    return answers::failed();
                };
                total = next;
            }
        }
        let Some(page) = Page::new(number, self.limits.page, total) else {
            return answers::failed();
        };
        let mut index = 0_u64;
        let mut rows = List::with_capacity(self.limits.page);
        for (id, row) in &self.repositories {
            if !self.installations.reaches(grant, *id) {
                continue;
            }
            if page.contains(index)
                && rows
                    .push(d::Reachable {
                        id: u64::from(*id),
                        owner: row.owner_id,
                        login: row.owner.clone(),
                        name: row.name.clone(),
                        private: row.private,
                    })
                    .is_err()
            {
                return answers::failed();
            }
            let Some(next) = index.checked_add(1) else {
                return answers::failed();
            };
            index = next;
        }
        page.link(answers::response(200, &Body::Reachable(rows.as_slice(), total), &self.limits), target, &self.limits)
    }
}
fn installation(row: &installations::Installation) -> d::Installation {
    d::Installation {
        id: row.id,
        login: row.login.clone(),
        account: row.account,
        kind: match row.kind {
            Kind::User => d::Account::User,
            Kind::Organization => d::Account::Organization,
        },
        created: row.created,
        suspended: row.suspended,
    }
}
pub(crate) fn joined(first: &[u8], last: &[u8]) -> Option<Box<[u8]>> {
    let mut out = Writer::new(first.len().checked_add(last.len())?);
    out.put(first).ok()?;
    out.put(last).ok()?;
    Some(out.finish())
}

/// Heap held by the tables plus one request's encoding/decoding scratch.
/// The caller owns and budgets domain replies, input bytes, returned responses
/// and outstanding `Pending` values; this service retains none of those.
#[must_use]
pub fn worst_case(limits: &Limits, config: &Config) -> Option<u64> {
    if github::limits::check(&limits.github).is_err()
        || limits.page == 0
        || limits.page > github::pin::PAGE_CAP
        || limits.page > limits.github.inner
        || limits.body == 0
        || limits.body > limits.github.body_bytes
        || limits.pages.states == 0
        || limits.pages.manifest_bytes == 0
        || limits.response < 128
    {
        return None;
    }
    let mut bytes = pages::worst_case(limits)?.checked_add(List::<Account>::worst_case(config.accounts.capacity())?)?;
    for account in &config.accounts {
        bytes = bytes.checked_add(u64::try_from(account.login.len()).ok()?)?;
    }
    bytes = bytes.checked_add(u64::try_from(config.person.len()).ok()?)?;
    bytes = bytes
        .checked_add(Installations::worst_case(&limits.installations)?)?
        .checked_add(Map::<u32, Row>::worst_case(limits.github.inner)?)?
        .checked_add(List::<Row>::worst_case(config.repositories.capacity())?)?;
    for row in &config.repositories {
        bytes = bytes
            .checked_add(u64::try_from(row.owner.len()).ok()?)?
            .checked_add(u64::try_from(row.name.len()).ok()?)?;
    }
    bytes = bytes
        .checked_add(u64::try_from(config.app.slug.len()).ok()?)?
        .checked_add(u64::try_from(config.app.webhook.len()).ok()?)?
        .checked_add(u64::from(limits.github.string_bytes))?;
    // The retained App and its GET /app snapshot coexist. A conversion's
    // manifest remains in the page store, whose budget counts its own lists.
    let app_lists = List::<d::Permission>::worst_case(limits.github.inner)?
        .checked_add(List::<Box<[u8]>>::worst_case(limits.github.inner)?)?
        .checked_add(
            u64::from(limits.github.string_bytes).checked_mul(2)?.checked_mul(u64::from(limits.github.inner))?,
        )?;
    bytes = bytes.checked_add(app_lists.checked_mul(2)?)?;
    // Compare moves file paths from the caller-owned domain reply into at
    // most 300 descriptors, alongside one page of commit ids. Those bounded
    // vectors coexist with the response encoder; file contents are dropped.
    bytes = bytes
        .checked_add(List::<d::File>::worst_case(300)?)?
        .checked_add(List::<skein_forge::api::Commit>::worst_case(limits.page)?)?;
    // The query reader retains at most 16 tokens and their decoded text
    // (bounded by its input), while its tokenizer and input intake coexist.
    bytes = bytes
        .checked_add(List::<skein_json::Token>::worst_case(16)?)?
        .checked_add(u64::from(limits.github.body_bytes))?;
    let row_bytes = u64::from(limits.github.string_bytes).checked_mul(4)?.checked_add(256)?;
    bytes
        .checked_add(row_bytes.checked_mul(u64::from(limits.page))?)?
        .checked_add(skein_json::tokenizer::worst_case(&limits.github.json)?)?
        .checked_add(List::<Box<[u8]>>::worst_case(limits.github.inner)?.checked_mul(2)?)?
        // The fixed JWT reader (512 tokens), its queues and signing scratch.
        .checked_add(65_536)?
        .checked_add(u64::from(limits.body).checked_mul(8)?)?
        .checked_add(u64::from(limits.response).checked_mul(8)?)?
        .checked_add(u64::from(limits.github.target_bytes).checked_mul(8)?)?
        .checked_add(u64::from(limits.github.string_bytes).checked_mul(8)?)
}
