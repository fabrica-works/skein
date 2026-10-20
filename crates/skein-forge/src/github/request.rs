//! GitHub's requests and their exact targets (forge-github.md, section 4.1).
use super::{id, limits::Limits};
use crate::api;
use alloc::boxed::Box;
use skein_lib::{Decimal, Wall, Writer};
/// An HTTP method.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}
/// The credential selected for the request.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Auth {
    Installation,
    App,
    None,
}
/// GitHub's representation requested by Accept.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Accept {
    Json,
    Raw,
}
/// One request at GitHub's pin, with repository path bytes from its row.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Request {
    Installations { page: u32 },
    Installation { id: u64 },
    Mint { installation: u64, scope: Scope },
    Reachable { page: u32, per_page: u32 },
    Viewer,
    App,
    HookConfig,
    SetHook { url: Box<[u8]> },
    Uninstall { id: u64 },
    Conversion { code: Box<[u8]> },
    BotUser { slug: Box<[u8]> },
    Repository { id: u64 },
    Locate { owner: Box<[u8]>, name: Box<[u8]> },
    Issues { owner: Box<[u8]>, name: Box<[u8]>, since: Wall, page: u32, per_page: u32 },
    Issue { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    Comments { owner: Box<[u8]>, name: Box<[u8]>, number: u64, page: u32, per_page: u32 },
    Pull { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    PullsFor { owner: Box<[u8]>, name: Box<[u8]>, head: Box<[u8]>, base: Box<[u8]>, open: bool },
    Reviews { owner: Box<[u8]>, name: Box<[u8]>, number: u64, page: u32, per_page: u32 },
    ReviewComments { owner: Box<[u8]>, name: Box<[u8]>, number: u64, review: u64, page: u32, per_page: u32 },
    CombinedStatus { owner: Box<[u8]>, name: Box<[u8]>, commit: api::Commit, page: u32, per_page: u32 },
    CheckRuns { owner: Box<[u8]>, name: Box<[u8]>, commit: api::Commit, page: u32, per_page: u32 },
    Job { owner: Box<[u8]>, name: Box<[u8]>, id: u64 },
    JobLog { owner: Box<[u8]>, name: Box<[u8]>, id: u64 },
    Branch { owner: Box<[u8]>, name: Box<[u8]>, branch: Box<[u8]> },
    Branches { owner: Box<[u8]>, name: Box<[u8]>, page: u32, per_page: u32 },
    PullFiles { owner: Box<[u8]>, name: Box<[u8]>, number: u64, page: u32, per_page: u32 },
    Compare { owner: Box<[u8]>, name: Box<[u8]>, before: api::Commit, after: api::Commit, per_page: u32 },
    Raw { owner: Box<[u8]>, name: Box<[u8]>, path: Box<[u8]>, commit: api::Commit },
    RulesForBranch { owner: Box<[u8]>, name: Box<[u8]>, branch: Box<[u8]>, page: u32 },
    Ruleset { owner: Box<[u8]>, name: Box<[u8]>, id: u64 },
    ClassicProtection { owner: Box<[u8]>, name: Box<[u8]>, branch: Box<[u8]> },
    Collaborators { owner: Box<[u8]>, name: Box<[u8]>, page: u32, per_page: u32 },
    Permission { owner: Box<[u8]>, name: Box<[u8]>, login: Box<[u8]> },
    User { id: u64 },
    Reopen { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    Edit { owner: Box<[u8]>, name: Box<[u8]>, number: u64, title: Option<Box<[u8]>>, body: Option<Box<[u8]>> },
    Close { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    DeleteBranch { owner: Box<[u8]>, name: Box<[u8]>, branch: Box<[u8]> },
}
/// The permissions requested from an installation's mint.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Scope {
    Full,
    Contents,
}
/// A request ready for the component's HTTP head.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Encoded {
    pub method: Method,
    pub target: Box<[u8]>,
    pub body: Option<Box<[u8]>>,
    pub accept: Accept,
    pub auth: Auth,
}
/// An input that cannot be encoded within the target or body's bound.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Refusal {
    Text,
    TooLong,
}
/// Measures, checks and writes a request's target and body.
pub fn encode(request: &Request, limits: &Limits) -> Result<Encoded, Refusal> {
    bounded_fields(request, limits)?;
    let mut out = Target { length: 0, writer: None };
    target(request, &mut out)?;
    if out.length > usize::try_from(limits.target_bytes).expect("u32 fits usize") {
        return Err(Refusal::TooLong);
    }
    let mut out = Target { length: 0, writer: Some(Writer::new(out.length)) };
    target(request, &mut out)?;
    let accept = match request {
        Request::Raw { .. } => Accept::Raw,
        Request::Installations { .. }
        | Request::Installation { .. }
        | Request::Mint { .. }
        | Request::Reachable { .. }
        | Request::Viewer
        | Request::App
        | Request::HookConfig
        | Request::SetHook { .. }
        | Request::Uninstall { .. }
        | Request::Conversion { .. }
        | Request::BotUser { .. }
        | Request::Repository { .. }
        | Request::Locate { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Branches { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::DeleteBranch { .. }
        | Request::Reopen { .. }
        | Request::Close { .. }
        | Request::Edit { .. }
        | Request::User { .. } => Accept::Json,
    };
    let (method, auth) = selection(request);
    Ok(Encoded {
        method,
        target: out.writer.expect("writing pass").finish(),
        body: body(request, limits)?,
        accept,
        auth,
    })
}
/// Accept and the API pin, beside the component's Host, User-Agent and Authorization.
#[must_use]
pub fn fields(encoded: &Encoded) -> [(&'static [u8], &'static [u8]); 2] {
    let accept: &[u8] = match encoded.accept {
        Accept::Json => b"application/vnd.github+json",
        Accept::Raw => b"application/vnd.github.raw+json",
    };
    [(b"Accept", accept), (b"X-GitHub-Api-Version", super::pin::API_VERSION.as_bytes())]
}
#[derive(Debug)]
struct Target {
    length: usize,
    writer: Option<Writer>,
}
impl Target {
    fn put(&mut self, bytes: &[u8]) {
        self.length = self.length.saturating_add(bytes.len());
        if let Some(out) = &mut self.writer {
            out.put(bytes).expect("measured target");
        }
    }
    fn number(&mut self, n: u64) {
        self.put(Decimal::of(n).as_bytes());
    }
    fn segment(&mut self, text: &[u8]) -> Result<(), Refusal> {
        self.text(text, false)
    }
    fn file(&mut self, text: &[u8]) -> Result<(), Refusal> {
        self.text(text, true)
    }
    fn text(&mut self, text: &[u8], slashes: bool) -> Result<(), Refusal> {
        if !valid_text(text) {
            return Err(Refusal::Text);
        }
        for byte in text {
            let size = if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) || (slashes && *byte == b'/') {
                1
            } else {
                3
            };
            self.length = self.length.saturating_add(size);
        }
        if let Some(out) = &mut self.writer {
            if slashes {
                crate::forgejo::path::file(text, out).expect("measured path");
            } else {
                crate::forgejo::path::segment(text, out).expect("measured segment");
            }
        }
        Ok(())
    }
    fn repo(&mut self, owner: &[u8], name: &[u8]) -> Result<(), Refusal> {
        self.put(b"/repos/");
        self.segment(owner)?;
        self.put(b"/");
        self.segment(name)
    }
    fn job(&mut self, owner: &[u8], name: &[u8], id: u64, suffix: &[u8]) -> Result<(), Refusal> {
        self.repo(owner, name)?;
        self.put(b"/actions/jobs/");
        self.number(id);
        self.put(suffix);
        Ok(())
    }
    fn commit(&mut self, commit: &api::Commit) -> Result<(), Refusal> {
        self.put(&id::write_commit(commit).ok_or(Refusal::Text)?);
        Ok(())
    }
    fn page(&mut self, page: u32, size: u32, query: bool) {
        self.put(if query { b"&per_page=" } else { b"?per_page=" });
        self.number(u64::from(size));
        self.put(b"&page=");
        self.number(u64::from(page));
    }
}
#[expect(clippy::disallowed_methods, reason = "the dialect is the protocol layer validating path and query text")]
pub(crate) fn valid_text(text: &[u8]) -> bool {
    core::str::from_utf8(text).is_ok()
}
#[expect(clippy::too_many_lines, reason = "exhaustive target dispatch includes read and write requests")]
fn target(request: &Request, out: &mut Target) -> Result<(), Refusal> {
    match request {
        Request::Installations { .. }
        | Request::Installation { .. }
        | Request::Mint { .. }
        | Request::Reachable { .. }
        | Request::Viewer
        | Request::App
        | Request::HookConfig
        | Request::SetHook { .. }
        | Request::Uninstall { .. }
        | Request::Conversion { .. }
        | Request::BotUser { .. } => return app_target(request, out),
        Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::PullFiles { .. }
        | Request::Branches { .. } => return items_target(request, out),
        Request::Edit { owner, name, number, .. }
        | Request::Close { owner, name, number }
        | Request::Reopen { owner, name, number } => {
            out.repo(owner, name)?;
            out.put(b"/issues/");
            out.number(*number);
        }
        Request::DeleteBranch { owner, name, branch } => {
            out.repo(owner, name)?;
            out.put(b"/git/refs/heads/");
            out.file(branch)?;
        }
        Request::Repository { id } => {
            out.put(b"/repositories/");
            out.number(*id);
        }
        Request::Locate { owner, name } => {
            out.repo(owner, name)?;
        }
        Request::CombinedStatus { owner, name, commit, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/commits/");
            out.commit(commit)?;
            out.put(b"/status");
            out.page(*page, *per_page, false);
        }
        Request::CheckRuns { owner, name, commit, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/commits/");
            out.commit(commit)?;
            out.put(b"/check-runs?filter=latest");
            out.page(*page, *per_page, true);
        }
        Request::Job { owner, name, id } => out.job(owner, name, *id, b"")?,
        Request::JobLog { owner, name, id } => out.job(owner, name, *id, b"/logs")?,
        Request::Branch { owner, name, branch } => {
            out.repo(owner, name)?;
            out.put(b"/branches/");
            out.file(branch)?;
        }
        Request::Compare { owner, name, before, after, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/compare/");
            out.commit(before)?;
            out.put(b"...");
            out.commit(after)?;
            out.put(b"?per_page=");
            out.number(u64::from(*per_page));
        }
        Request::Raw { owner, name, path, commit } => {
            out.repo(owner, name)?;
            out.put(b"/contents/");
            out.file(path)?;
            out.put(b"?ref=");
            out.commit(commit)?;
        }
        Request::RulesForBranch { owner, name, branch, page } => {
            out.repo(owner, name)?;
            out.put(b"/rules/branches/");
            out.file(branch)?;
            out.page(*page, super::pin::PAGE_CAP, false);
        }
        Request::Ruleset { owner, name, id } => {
            out.repo(owner, name)?;
            out.put(b"/rulesets/");
            out.number(*id);
        }
        Request::ClassicProtection { owner, name, branch } => {
            out.repo(owner, name)?;
            out.put(b"/branches/");
            out.file(branch)?;
            out.put(b"/protection");
        }
        Request::Collaborators { owner, name, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/collaborators?affiliation=all");
            out.page(*page, *per_page, true);
        }
        Request::Permission { owner, name, login } => {
            out.repo(owner, name)?;
            out.put(b"/collaborators/");
            out.segment(login)?;
            out.put(b"/permission");
        }
        Request::User { id } => {
            out.put(b"/user/");
            out.number(*id);
        }
    }
    Ok(())
}
fn items_target(request: &Request, out: &mut Target) -> Result<(), Refusal> {
    match request {
        Request::Issues { owner, name, since, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/issues?state=all&since=");
            out.segment(&crate::forgejo::time::write(*since))?;
            out.put(b"&sort=updated&direction=asc");
            out.page(*page, *per_page, true);
        }
        Request::Issue { owner, name, number } => {
            out.repo(owner, name)?;
            out.put(b"/issues/");
            out.number(*number);
        }
        Request::Comments { owner, name, number, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/issues/");
            out.number(*number);
            out.put(b"/comments");
            out.page(*page, *per_page, false);
        }
        Request::Pull { owner, name, number } => {
            out.repo(owner, name)?;
            out.put(b"/pulls/");
            out.number(*number);
        }
        Request::PullsFor { owner, name, head, base, open } => {
            out.repo(owner, name)?;
            out.put(if *open { b"/pulls?state=open&head=" } else { b"/pulls?state=all&head=" });
            out.segment(owner)?;
            out.put(b"%3A");
            out.segment(head)?;
            out.put(b"&base=");
            out.segment(base)?;
            if !*open {
                out.put(b"&sort=created&direction=asc");
            }
            out.put(b"&per_page=1");
        }
        Request::Reviews { owner, name, number, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/pulls/");
            out.number(*number);
            out.put(b"/reviews");
            out.page(*page, *per_page, false);
        }
        Request::ReviewComments { owner, name, number, review, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/pulls/");
            out.number(*number);
            out.put(b"/reviews/");
            out.number(*review);
            out.put(b"/comments");
            out.page(*page, *per_page, false);
        }
        Request::PullFiles { owner, name, number, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/pulls/");
            out.number(*number);
            out.put(b"/files");
            out.page(*page, *per_page, false);
        }
        Request::Branches { owner, name, page, per_page } => {
            out.repo(owner, name)?;
            out.put(b"/branches");
            out.page(*page, *per_page, false);
        }
        Request::Installations { .. }
        | Request::Installation { .. }
        | Request::Mint { .. }
        | Request::Reachable { .. }
        | Request::Viewer
        | Request::App
        | Request::HookConfig
        | Request::SetHook { .. }
        | Request::Uninstall { .. }
        | Request::Conversion { .. }
        | Request::BotUser { .. }
        | Request::Repository { .. }
        | Request::Locate { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Compare { .. }
        | Request::Raw { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::DeleteBranch { .. }
        | Request::Reopen { .. }
        | Request::Close { .. }
        | Request::Edit { .. }
        | Request::User { .. } => unreachable!("item target dispatch"),
    }
    Ok(())
}
/// Decodes percent escapes after the caller has bounded the input.
pub(crate) fn unescape(text: &[u8]) -> Option<Box<[u8]>> {
    let mut out = skein_lib::List::with_capacity(u32::try_from(text.len()).ok()?);
    let mut iter = text.iter();
    for _ in 0..text.len() {
        let Some(byte) = iter.next() else {
            break;
        };
        let byte = if *byte == b'%' {
            let high = escape_digit(*iter.next()?)?;
            let low = escape_digit(*iter.next()?)?;
            high.checked_mul(16)?.checked_add(low)?
        } else {
            *byte
        };
        out.push(byte).ok()?;
    }
    let out = out.into_boxed();
    if valid_text(&out) { Some(out) } else { None }
}
fn escape_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => byte.checked_sub(b'0'),
        b'a'..=b'f' => byte.checked_sub(b'a')?.checked_add(10),
        b'A'..=b'F' => byte.checked_sub(b'A')?.checked_add(10),
        _ => None,
    }
}

fn selection(request: &Request) -> (Method, Auth) {
    match request {
        Request::Edit { .. } | Request::Close { .. } | Request::Reopen { .. } => (Method::Patch, Auth::Installation),
        Request::DeleteBranch { .. } => (Method::Delete, Auth::Installation),
        Request::Installations { .. } | Request::Installation { .. } | Request::App | Request::HookConfig => {
            (Method::Get, Auth::App)
        }
        Request::Mint { .. } => (Method::Post, Auth::App),
        Request::SetHook { .. } => (Method::Patch, Auth::App),
        Request::Uninstall { .. } => (Method::Delete, Auth::App),
        Request::Conversion { .. } => (Method::Post, Auth::None),
        Request::BotUser { .. } => (Method::Get, Auth::None),
        Request::Viewer => (Method::Post, Auth::Installation),
        Request::Reachable { .. }
        | Request::Repository { .. }
        | Request::Locate { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Branches { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::Raw { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::User { .. } => (Method::Get, Auth::Installation),
    }
}
fn body(request: &Request, limits: &Limits) -> Result<Option<Box<[u8]>>, Refusal> {
    match request {
        Request::Mint { scope: Scope::Contents, .. }
        | Request::Viewer
        | Request::SetHook { .. }
        | Request::Reopen { .. }
        | Request::Close { .. }
        | Request::Edit { .. } => {}
        Request::Mint { scope: Scope::Full, .. }
        | Request::Installations { .. }
        | Request::Installation { .. }
        | Request::Reachable { .. }
        | Request::App
        | Request::HookConfig
        | Request::Uninstall { .. }
        | Request::Conversion { .. }
        | Request::BotUser { .. }
        | Request::Repository { .. }
        | Request::Locate { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Branches { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::Raw { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::DeleteBranch { .. }
        | Request::User { .. } => return Ok(None),
    }
    let bounds = skein_json::writer::Limits { depth: super::limits::DEPTH, length: limits.body_bytes };
    let mut measure = skein_json::writer::Encoder::measure(&bounds);
    body_json(request, &mut measure);
    let length = match measure.measured() {
        Ok(length) => length,
        Err(skein_json::writer::Refusal::Text) => return Err(Refusal::Text),
        Err(
            skein_json::writer::Refusal::TooLong
            | skein_json::writer::Refusal::TooDeep
            | skein_json::writer::Refusal::Number,
        ) => return Err(Refusal::TooLong),
    };
    let mut out = skein_json::writer::Encoder::write(length, &bounds);
    body_json(request, &mut out);
    Ok(Some(out.finish()))
}
fn body_json(request: &Request, json: &mut skein_json::writer::Encoder) {
    json.object_start();
    match request {
        Request::Edit { title, body, .. } => {
            if let Some(title) = title {
                json.key(b"title");
                json.string(title);
            }
            if let Some(body) = body {
                json.key(b"body");
                json.string(body);
            }
        }

        Request::Reopen { .. } => {
            json.key(b"state");
            json.string(b"open");
        }
        Request::Close { .. } => {
            json.key(b"state");
            json.string(b"closed");
        }
        Request::Mint { .. } => {
            json.key(b"permissions");
            json.object_start();
            json.key(b"contents");
            json.string(b"write");
            json.object_end();
        }
        Request::Viewer => {
            json.key(b"query");
            json.string(super::document::VIEWER);
        }
        Request::SetHook { url } => {
            json.key(b"url");
            json.string(url);
        }
        Request::Installations { .. }
        | Request::Installation { .. }
        | Request::Reachable { .. }
        | Request::App
        | Request::HookConfig
        | Request::Uninstall { .. }
        | Request::Conversion { .. }
        | Request::BotUser { .. }
        | Request::Repository { .. }
        | Request::Locate { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Branches { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::Raw { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::DeleteBranch { .. }
        | Request::User { .. } => unreachable!("request has a body"),
    }
    json.object_end();
}

fn app_target(request: &Request, out: &mut Target) -> Result<(), Refusal> {
    match request {
        Request::Installations { page } => {
            out.put(b"/app/installations");
            out.page(*page, super::pin::PAGE_CAP, false);
        }
        Request::Installation { id } | Request::Uninstall { id } => {
            out.put(b"/app/installations/");
            out.number(*id);
        }
        Request::Mint { installation, .. } => {
            out.put(b"/app/installations/");
            out.number(*installation);
            out.put(b"/access_tokens");
        }
        Request::Reachable { page, per_page } => {
            if !(1..=super::pin::PAGE_CAP).contains(per_page) {
                return Err(Refusal::Text);
            }
            out.put(b"/installation/repositories");
            out.page(*page, *per_page, false);
        }
        Request::Viewer => out.put(b"/graphql"),
        Request::App => out.put(b"/app"),
        Request::HookConfig | Request::SetHook { .. } => out.put(b"/app/hook/config"),
        Request::Conversion { code } => {
            out.put(b"/app-manifests/");
            out.segment(code)?;
            out.put(b"/conversions");
        }
        Request::BotUser { slug } => {
            out.put(b"/users/");
            out.segment(slug)?;
            out.put(b"%5Bbot%5D");
        }
        Request::Repository { .. }
        | Request::Locate { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Branches { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::Raw { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::DeleteBranch { .. }
        | Request::Reopen { .. }
        | Request::Close { .. }
        | Request::Edit { .. }
        | Request::User { .. } => unreachable!("App target dispatch"),
    }
    Ok(())
}

fn bounded(text: &[u8], limit: u32) -> Result<(), Refusal> {
    if u64::try_from(text.len()).ok().ok_or(Refusal::TooLong)? > u64::from(limit) {
        return Err(Refusal::TooLong);
    }
    Ok(())
}

fn bounded_fields(request: &Request, limits: &Limits) -> Result<(), Refusal> {
    match request {
        Request::Edit { title, body, .. } => {
            if let Some(title) = title {
                bounded(title, limits.string_bytes)?;
            }
            if let Some(body) = body {
                bounded(body, limits.body_bytes)?;
            }
        }

        Request::DeleteBranch { branch, .. }
            if u64::try_from(branch.len()).ok().ok_or(Refusal::TooLong)? > u64::from(limits.string_bytes) =>
        {
            return Err(Refusal::TooLong);
        }
        Request::DeleteBranch { .. }
        | Request::Raw { .. }
        | Request::Installations { .. }
        | Request::Installation { .. }
        | Request::Mint { .. }
        | Request::Reachable { .. }
        | Request::Viewer
        | Request::App
        | Request::HookConfig
        | Request::SetHook { .. }
        | Request::Uninstall { .. }
        | Request::Conversion { .. }
        | Request::BotUser { .. }
        | Request::Repository { .. }
        | Request::Locate { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Branches { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::Reopen { .. }
        | Request::Close { .. }
        | Request::User { .. } => {}
    }
    Ok(())
}
