//! The setup documents written to the browser (forge-github.md, 11).

use alloc::boxed::Box;
use skein_json::writer::{self, Encoder};
use skein_lib::{List, Writer, bytes};

pub use super::document::Access;
use super::document::{App, Permission};
/// The App's permissions (forge-github.md, 11).
pub const PERMISSIONS: [(&[u8], Access); 9] = [
    (b"contents", Access::Write),
    (b"pull_requests", Access::Write),
    (b"issues", Access::Write),
    (b"merge_queues", Access::Write),
    (b"checks", Access::Read),
    (b"statuses", Access::Read),
    (b"actions", Access::Read),
    (b"administration", Access::Read),
    (b"metadata", Access::Read),
];

/// The subscribed events; installation events arrive without subscription.
pub const EVENTS: [&[u8]; 14] = [
    b"issues",
    b"issue_comment",
    b"pull_request",
    b"pull_request_review",
    b"pull_request_review_comment",
    b"push",
    b"create",
    b"delete",
    b"check_run",
    b"check_suite",
    b"status",
    b"workflow_job",
    b"workflow_run",
    b"merge_group",
];

/// Wanted permissions and subscriptions absent from an App's grants.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Missing {
    pub permissions: Box<[Permission]>,
    pub events: Box<[Box<[u8]>]>,
}

/// Names every wanted entry the App lacks, in manifest order.
/// Admin covers write and read; write covers read.
#[must_use]
pub fn lacks(app: &App) -> Missing {
    let mut permissions = List::with_capacity(u32::try_from(PERMISSIONS.len()).expect("constant count"));
    for (name, access) in PERMISSIONS {
        let mut held = false;
        for permission in &app.permissions {
            if permission.name.as_ref() == name && permission.access.satisfies(access) {
                held = true;
            }
        }
        if !held {
            permissions.push(Permission { name: bytes::copy_of(name), access }).expect("wanted count");
        }
    }
    let mut events = List::with_capacity(u32::try_from(EVENTS.len()).expect("constant count"));
    for name in EVENTS {
        let mut held = false;
        for event in &app.events {
            if event.as_ref() == name {
                held = true;
            }
        }
        if !held {
            events.push(bytes::copy_of(name)).expect("wanted count");
        }
    }
    Missing { permissions: permissions.into_boxed(), events: events.into_boxed() }
}

/// The manifest's caller-supplied text, unescaped.
#[derive(Clone, Copy, Debug)]
pub struct Values<'a> {
    pub name: &'a [u8],
    pub url: &'a [u8],
    pub hook_url: &'a [u8],
    pub redirect_url: &'a [u8],
    pub setup_url: &'a [u8],
}

/// The manifest's compact JSON, measured before allocation.
pub fn manifest(values: &Values<'_>, limits: &writer::Limits) -> Result<Box<[u8]>, writer::Refusal> {
    let mut measure = Encoder::measure(limits);
    encode_manifest(&mut measure, values);
    let length = measure.measured()?;
    let mut write = Encoder::write(length, limits);
    encode_manifest(&mut write, values);
    Ok(write.finish())
}

fn encode_manifest(json: &mut Encoder, values: &Values<'_>) {
    json.object_start();
    for (key, value) in [(b"name".as_slice(), values.name), (b"url", values.url)] {
        json.key(key);
        json.string(value);
    }
    json.key(b"public");
    json.boolean(true);
    json.key(b"redirect_url");
    json.string(values.redirect_url);
    json.key(b"setup_url");
    json.string(values.setup_url);
    json.key(b"setup_on_update");
    json.boolean(true);
    json.key(b"hook_attributes");
    json.object_start();
    json.key(b"url");
    json.string(values.hook_url);
    json.key(b"active");
    json.boolean(true);
    json.object_end();
    json.key(b"default_permissions");
    json.object_start();
    for (name, access) in PERMISSIONS {
        json.key(name);
        json.string(access.text());
    }
    json.object_end();
    json.key(b"default_events");
    json.array_start();
    for event in EVENTS {
        json.string(event);
    }
    json.array_end();
    json.object_end();
}

/// The account that will own the App.
#[derive(Clone, Copy, Debug)]
pub enum Account<'a> {
    Person,
    Organization(&'a [u8]),
}

/// The personal or organization's App creation target, including state.
/// `web` is an origin without a trailing slash; state and login must be
/// nonempty URI unreserved bytes. Refuses a result past `max`.
#[must_use]
pub fn form_target(web: &[u8], account: &Account<'_>, state: &[u8], max: u32) -> Option<Box<[u8]>> {
    if web.is_empty() || web.ends_with(b"/") || !unreserved(state) {
        return None;
    }
    match account {
        Account::Person => join(&[web, b"/settings/apps/new?state=", state], max),
        Account::Organization(login) => {
            if !unreserved(login) {
                return None;
            }
            join(&[web, b"/organizations/", login, b"/settings/apps/new?state=", state], max)
        }
    }
}

/// An HTML form posting the manifest to `target`, with only its submission
/// script and no external resources. Both attribute values escape `&<>"'`.
/// Refuses a document past `max` before allocating it.
#[must_use]
pub fn form_page(target: &[u8], manifest: &[u8], max: u32) -> Option<Box<[u8]>> {
    let start = b"<!doctype html><meta charset=\"utf-8\"><title>GitHub App</title><form method=\"post\" action=\"";
    let middle = b"\"><input type=\"hidden\" name=\"manifest\" value=\"";
    let end = b"\"></form><script>document.forms[0].submit()</script>";
    let length = start
        .len()
        .checked_add(attribute_length(target)?)?
        .checked_add(middle.len())?
        .checked_add(attribute_length(manifest)?)?
        .checked_add(end.len())?;
    if length > usize::try_from(max).ok()? {
        return None;
    }
    let mut out = Writer::new(length);
    out.put(start).ok()?;
    put_attribute(&mut out, target)?;
    out.put(middle).ok()?;
    put_attribute(&mut out, manifest)?;
    out.put(end).ok()?;
    Some(out.finish())
}

/// The installation target, including state. `web` has no trailing slash;
/// slug and state must be nonempty URI unreserved bytes.
/// Refuses a result past `max`.
#[must_use]
pub fn install_page(web: &[u8], slug: &[u8], state: &[u8], max: u32) -> Option<Box<[u8]>> {
    if web.is_empty() || web.ends_with(b"/") || !unreserved(slug) || !unreserved(state) {
        return None;
    }
    join(&[web, b"/apps/", slug, b"/installations/new?state=", state], max)
}

fn unreserved(text: &[u8]) -> bool {
    if text.is_empty() {
        return false;
    }
    for byte in text {
        if !byte.is_ascii_alphanumeric() && !b"._~-".contains(byte) {
            return false;
        }
    }
    true
}

fn join(parts: &[&[u8]], max: u32) -> Option<Box<[u8]>> {
    let mut length = 0_usize;
    for part in parts {
        length = length.checked_add(part.len())?;
    }
    if length > usize::try_from(max).ok()? {
        return None;
    }
    let mut out = Writer::new(length);
    for part in parts {
        out.put(part).ok()?;
    }
    Some(out.finish())
}

fn escaped(byte: u8) -> Option<&'static [u8]> {
    match byte {
        b'&' => Some(b"&amp;"),
        b'<' => Some(b"&lt;"),
        b'>' => Some(b"&gt;"),
        b'"' => Some(b"&quot;"),
        b'\'' => Some(b"&#39;"),
        _ => None,
    }
}

fn attribute_length(text: &[u8]) -> Option<usize> {
    let mut length = 0_usize;
    for &byte in text {
        let width = match escaped(byte) {
            Some(entity) => entity.len(),
            None => 1,
        };
        length = length.checked_add(width)?;
    }
    Some(length)
}

fn put_attribute(out: &mut Writer, text: &[u8]) -> Option<()> {
    for &byte in text {
        match escaped(byte) {
            Some(entity) => out.put(entity).ok()?,
            None => out.put(&[byte]).ok()?,
        }
    }
    Some(())
}
