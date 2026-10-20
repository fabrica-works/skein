//! Bounded documents and GitHub's refusal texts.
use crate::service::{Limits, Response};
use alloc::boxed::Box;
use skein_forge::github::{document as d, server};
use skein_lib::{Overflow, Writer, bytes};

pub(crate) const SUSPENDED: &[u8] = b"This installation has been suspended";

#[derive(Debug)]
pub(crate) enum Body<'a> {
    App(&'a d::App),
    AppIdentity(&'a d::App),
    BotUser(u64, &'a [u8]),
    Hook(&'a d::Hook),
    Installation(&'a d::Installation),
    Installations(&'a [d::Installation]),
    Minted(&'a d::Minted),
    Reachable(&'a [d::Reachable], u64),
    Viewer(&'a d::Viewer),
    Graphql,
    Repository(&'a d::Repository),
    Branch(&'a d::Branch),
    Branches(&'a [&'a [u8]]),
    Issues(&'a [d::Issue]),
    Issue(&'a d::Issue),
    Comments(&'a [d::Comment]),
    PullGraphql(&'a d::PullGraphql),
    Pull(&'a d::Pull),
    Pulls(&'a [d::Pull]),
    Files(&'a [d::File]),
    Compare(&'a d::Compare, &'a [skein_forge::api::Commit], &'a [d::File]),
    User(&'a d::User),
    Collaborators(&'a [d::Collaborator]),
    Permission(skein_forge::api::Permission),
    Reviews(&'a [d::Review]),
    Remarks(&'a [d::Remark]),
    Refusal(&'a [u8]),
}
fn encode(body: &Body<'_>, out: &mut Writer) -> Result<(), Overflow> {
    match body {
        Body::BotUser(id, slug) => server::bot_user(*id, slug, out),
        Body::App(row) => server::app(row, out),
        Body::AppIdentity(row) => server::app_identity(row, out),
        Body::Hook(row) => server::hook(row, out),
        Body::Installation(row) => server::installation(row, out),
        Body::Installations(rows) => server::installations(rows, out),
        Body::Minted(row) => server::minted(row, out),
        Body::Reachable(rows, total) => server::reachable(rows, *total, out),
        Body::Viewer(row) => server::viewer(row, out),
        Body::Graphql => server::graphql_refusal(b"UNPROCESSABLE", b"Unsupported query", out),
        Body::Repository(row) => server::repository(row, out),
        Body::Branch(row) => server::branch(row, out),
        Body::Branches(rows) => server::branches(rows, out),
        Body::Issues(rows) => server::issues(rows, out),
        Body::Issue(row) => server::issue(row, out),
        Body::Comments(rows) => server::comments(rows, out),
        Body::PullGraphql(row) => server::pull_graphql(row, out),
        Body::Pull(row) => server::pull(row, out),
        Body::Pulls(rows) => server::pulls(rows, out),
        Body::Files(rows) => server::file_rows(rows, out),
        Body::Compare(row, commits, files) => server::compare(row, commits, files, out),
        Body::User(row) => server::user(row, out),
        Body::Collaborators(rows) => server::collaborators(rows, out),
        Body::Permission(value) => server::permission(*value, out),
        Body::Reviews(rows) => server::reviews(rows, out),
        Body::Remarks(rows) => server::remarks(rows, out),
        Body::Refusal(message) => server::refusal(message, &[], out),
    }
}
// Writer is exact-sized. Pad the scratch writer, then retain only the encoded
// prefix; both allocations are included in worst_case's response scratch.
pub(crate) fn finish(mut out: Writer) -> Option<Box<[u8]>> {
    let length = out.written();
    let padding = [0; 128];
    for _chunk in 0..out.room().div_euclid(padding.len()) {
        out.put(&padding).ok()?;
    }
    let tail = out.room();
    out.put(padding.get(..tail)?).ok()?;
    let bytes = out.finish();
    Some(bytes::copy_of(bytes.get(..length)?))
}
pub(crate) fn response(status: u16, body: &Body<'_>, limits: &Limits) -> Response {
    let Ok(capacity) = usize::try_from(limits.response) else {
        return failed();
    };
    let mut out = Writer::new(capacity);
    if encode(body, &mut out).is_err() {
        return failed();
    }
    match finish(out) {
        Some(body) => Response { status, link: None, body },
        None => failed(),
    }
}
pub(crate) fn refusal(status: u16, message: &[u8], limits: &Limits) -> Response {
    response(status, &Body::Refusal(message), limits)
}
pub(crate) fn missing(limits: &Limits) -> Response {
    refusal(404, b"Not Found", limits)
}
pub(crate) fn unauthorized(limits: &Limits) -> Response {
    refusal(401, b"Bad credentials", limits)
}
pub(crate) fn failed() -> Response {
    // new requires room for this terminal even when another encoder refuses.
    Response { status: 500, link: None, body: bytes::copy_of(br#"{"message":"Server Error","errors":[]}"#) }
}

#[derive(Debug)]
pub(crate) struct Page {
    start: u64,
    end: u64,
    next: Option<u32>,
    last: u32,
}
impl Page {
    pub(crate) fn new(page: u32, size: u32, total: u64) -> Option<Self> {
        let start = u64::from(page.checked_sub(1)?).checked_mul(u64::from(size))?;
        let end = start.checked_add(u64::from(size))?;
        let last =
            u32::try_from(total.checked_add(u64::from(size.checked_sub(1)?))?.checked_div(u64::from(size))?).ok()?;
        let next = if end < total { Some(page.checked_add(1)?) } else { None };
        Some(Self { start, end, next, last })
    }
    pub(crate) fn contains(&self, index: u64) -> bool {
        index >= self.start && index < self.end
    }
    pub(crate) fn link_next(&self, response: Response, target: &[u8], limits: &Limits) -> Response {
        let mut response = self.link(response, target, limits);
        if let Some(link) = &response.link
            && let Some(end) = bytes::find(link, b", ")
        {
            response.link = Some(bytes::copy_of(link.get(..end).expect("link delimiter")));
        }
        response
    }
    pub(crate) fn link(&self, mut response: Response, target: &[u8], limits: &Limits) -> Response {
        if response.status != 200 {
            return response;
        }
        if let Some(next) = self.next {
            let Ok(capacity) = usize::try_from(limits.response) else {
                return failed();
            };
            let mut out = Writer::new(capacity);
            if server::link(target, next, self.last, &mut out).is_err() {
                return failed();
            }
            let Some(link) = finish(out) else {
                return failed();
            };
            response.link = Some(link);
        }
        response
    }
}
