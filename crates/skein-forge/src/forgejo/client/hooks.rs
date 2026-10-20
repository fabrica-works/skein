use super::{items::Target, setup::SetupKind};
use crate::forgejo::{
    decode::{
        Malformed, Step,
        walk::{Field, Kind, Move, Read, Value, Walk},
    },
    document::{Document, Hook, Row},
    limits::Limits,
    request::{HookSpec, Page},
};
use alloc::boxed::Box;
use skein_json::{tokenizer, writer::Encoder};
use skein_lib::List;
const fn field(key: &'static [u8], read: Read, required: bool) -> Field {
    Field { key, read, required }
}
pub(crate) fn fields(kind: SetupKind) -> &'static [Field] {
    match kind {
        SetupKind::Hook => {
            const {
                &[
                    field(b"id", Read::Number, true),
                    field(b"config", Read::Object(Kind::Setup(SetupKind::Config)), true),
                    field(b"events", Read::Array(Kind::Setup(SetupKind::Events)), true),
                    field(b"active", Read::Boolean, true),
                ]
            }
        }
        SetupKind::Config => const { &[field(b"url", Read::Text, true), field(b"content_type", Read::Text, false)] },
        SetupKind::Hooks
        | SetupKind::Events
        | SetupKind::Event
        | SetupKind::Account
        | SetupKind::Org
        | SetupKind::Team
        | SetupKind::Teams
        | SetupKind::Units
        | SetupKind::Token
        | SetupKind::Tokens
        | SetupKind::Scopes
        | SetupKind::Scope => &[],
    }
}
pub(super) fn target(out: &mut Target, org: &[u8], page: Option<Page>, id: Option<u64>) {
    out.put(b"/api/v1/orgs/");
    out.segment(org);
    out.put(b"/hooks");
    if let Some(page) = page {
        out.page(page, false);
    }
    if let Some(id) = id {
        out.put(b"/");
        out.number(id);
    }
}
pub(super) fn repo_target(out: &mut Target, at: &crate::forgejo::request::Repo, page: Option<Page>, id: Option<u64>) {
    out.repo(at);
    out.put(b"/hooks");
    if let Some(page) = page {
        out.page(page, false);
    }
    if let Some(id) = id {
        out.put(b"/");
        out.number(id);
    }
}
pub(crate) fn specification(json: &mut Encoder, hook: &HookSpec, create: bool) {
    json.object_start();
    if create {
        json.key(b"type");
        json.string(b"forgejo");
    }
    json.key(b"config");
    json.object_start();
    for (key, value) in
        [(b"url".as_slice(), hook.url.as_ref()), (b"content_type", b"json"), (b"secret", hook.secret.expose())]
    {
        json.key(key);
        json.string(value);
    }
    json.object_end();
    events(json, &hook.events, hook.active);
    json.object_end();
}
pub(crate) fn events(json: &mut Encoder, events: &[Box<[u8]>], active: bool) {
    json.key(b"events");
    json.array_start();
    for event in events {
        json.string(event);
    }
    json.array_end();
    json.key(b"active");
    json.boolean(active);
}
pub(crate) fn empty() -> Hook {
    Hook { id: 0, url: Box::from([]), events: Box::from([]), active: false }
}
#[derive(Debug)]
pub(super) struct Hooks {
    kind: SetupKind,
    hook: Hook,
    events: List<Box<[u8]>>,
}
impl Hooks {
    pub(super) fn new(kind: SetupKind, limits: &Limits) -> Self {
        Self { kind, hook: empty(), events: List::with_capacity(limits.inner) }
    }
    pub(super) fn step(&mut self, movement: Move, limits: &Limits, walk: &Walk) -> Step {
        match movement {
            Move::Demand(demand) => {
                return Step::Demand(if walk.parent() == Some(Kind::Setup(SetupKind::Events)) {
                    tokenizer::Request::Text(limits.string_bytes)
                } else {
                    demand
                });
            }
            Move::Scalar { key, value, .. } => match (key, value) {
                (b"id", Value::Number(id)) => self.hook.id = id,
                (b"url", Value::Text(text)) => self.hook.url = text,
                (b"content_type", Value::Text(text)) if text.as_ref() == b"json" => {}
                (b"content_type", Value::Text(_)) => return Step::Malformed(Malformed::Value { field: key }),
                (b"active", Value::Boolean(value)) => self.hook.active = value,
                (b"events", Value::Text(text)) => {
                    if self.events.push(text).is_err() {
                        return Step::Malformed(Malformed::Bound { field: key });
                    }
                }
                _ => return Step::Malformed(Malformed::Shape { field: key }),
            },
            Move::End(Kind::Setup(SetupKind::Events)) => {
                self.hook.events = core::mem::replace(&mut self.events, List::with_capacity(limits.inner)).into_boxed();
            }
            Move::End(Kind::Setup(SetupKind::Hook)) if self.kind == SetupKind::Hooks => {
                return Step::Row(Row::Hook(core::mem::replace(&mut self.hook, empty())), tokenizer::Request::Next);
            }
            Move::End(_) => {}
            Move::Done => {
                return Step::Done(if self.kind == SetupKind::Hooks {
                    Document::Listed { skipped: 0 }
                } else {
                    Document::Hook(core::mem::replace(&mut self.hook, empty()))
                });
            }
        }
        Step::Demand(if walk.parent() == Some(Kind::Setup(SetupKind::Events)) {
            tokenizer::Request::Text(limits.string_bytes)
        } else {
            tokenizer::Request::Next
        })
    }
}
