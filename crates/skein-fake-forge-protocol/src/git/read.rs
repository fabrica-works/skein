use super::{Answer, Call, Decoder, Name, Object, Parents};
use crate::Limits;
use alloc::boxed::Box;
use skein_fake_forge_domain::api;
use skein_json::{Compact, Document, Kind, Token, document, tokenizer};
use skein_lib::{List, Time, base64, bytes};

pub(super) fn collect(decoder: &mut Decoder, event: &tokenizer::Event) -> Result<Option<Document>, ()> {
    if decoder.over {
        return Err(());
    }
    match event {
        tokenizer::Event::Token(token) => {
            let (kind, text): (Kind, &[u8]) = match token {
                Token::ObjectStart => (Kind::ObjectStart, b""),
                Token::ObjectEnd => (Kind::ObjectEnd, b""),
                Token::ArrayStart => (Kind::ArrayStart, b""),
                Token::ArrayEnd => (Kind::ArrayEnd, b""),
                Token::Key(text) => (Kind::Key, text),
                Token::String(text) => (Kind::String, text),
                Token::Number(text) => (Kind::Number, text),
                Token::True => (Kind::True, b""),
                Token::False => (Kind::False, b""),
                Token::Null => (Kind::Null, b""),
            };
            let len = u32::try_from(text.len()).or(Err(()))?;
            if len > decoder.text.room() || decoder.records.room() == 0 {
                decoder.over = true;
                return Err(());
            }
            let start = if text.is_empty() { 0 } else { decoder.text.len() };
            for byte in text {
                decoder.text.push(*byte).expect("admitted text");
            }
            decoder.records.push(Compact { kind, start, len }).expect("admitted record");
            Ok(None)
        }
        tokenizer::Event::Done => {
            decoder.over = true;
            let limits = document::Limits { tokens: decoder.records.len(), text: decoder.text.len() };
            let records = core::mem::replace(&mut decoder.records, List::with_capacity(0)).into_boxed();
            let text = core::mem::replace(&mut decoder.text, List::with_capacity(0)).into_boxed();
            Ok(Some(Document::from_parts(text, records, &limits).or(Err(()))?))
        }
        tokenizer::Event::Long(_)
        | tokenizer::Event::Skipped(_)
        | tokenizer::Event::Failed(_)
        | tokenizer::Event::Closed => {
            decoder.over = true;
            Err(())
        }
    }
}
fn kind(doc: &Document, at: u32) -> Result<Kind, ()> {
    Ok(doc.token(at).ok_or(())?.kind)
}
fn after(doc: &Document, at: u32) -> Result<u32, ()> {
    let mut depth = 0_u32;
    for index in at..doc.len() {
        match kind(doc, index)? {
            Kind::ObjectStart | Kind::ArrayStart => depth = depth.checked_add(1).ok_or(())?,
            Kind::ObjectEnd | Kind::ArrayEnd => depth = depth.checked_sub(1).ok_or(())?,
            Kind::Key | Kind::String | Kind::Number | Kind::True | Kind::False | Kind::Null | Kind::Long => {}
        }
        if depth == 0 {
            return index.checked_add(1).ok_or(());
        }
    }
    Err(())
}
fn text(doc: &Document, at: u32, expected: Kind) -> Result<&[u8], ()> {
    let record = doc.token(at).ok_or(())?;
    if record.kind != expected {
        return Err(());
    }
    doc.text(record).ok_or(())
}
fn number(doc: &Document, at: u32) -> Result<u64, ()> {
    let value = text(doc, at, Kind::Number)?;
    if value.is_empty() {
        return Err(());
    }
    let mut result = 0_u64;
    for digit in value {
        if !digit.is_ascii_digit() {
            return Err(());
        }
        result = result.checked_mul(10).ok_or(())?.checked_add(u64::from(digit.saturating_sub(b'0'))).ok_or(())?;
    }
    Ok(result)
}
fn optional(doc: &Document, at: u32) -> Result<Option<u64>, ()> {
    if kind(doc, at)? == Kind::Null { Ok(None) } else { Ok(Some(number(doc, at)?)) }
}
fn string(doc: &Document, at: u32, max: u32) -> Result<Box<[u8]>, ()> {
    let value = text(doc, at, Kind::String)?;
    if value.len() > usize::try_from(max).expect("u32 fits") {
        return Err(());
    }
    Ok(bytes::copy_of(value))
}
fn content(doc: &Document, at: u32, max: u32) -> Result<Box<[u8]>, ()> {
    base64::decode(base64::Alphabet::Standard, text(doc, at, Kind::String)?, max).or(Err(()))
}
// A schema has at most eight members. Slots are on the stack and contain
// positions only; no allocation or scan grows with a peer's member count.
struct Members {
    values: [Option<u32>; 8],
}
impl Members {
    fn at(&self, slot: usize) -> Result<u32, ()> {
        self.find(slot).ok_or(())
    }
    fn find(&self, slot: usize) -> Option<u32> {
        self.values.get(slot).copied().flatten()
    }
}
fn object(doc: &Document, at: u32, required: &[&[u8]], optional: &[&[u8]]) -> Result<Members, ()> {
    if kind(doc, at)? != Kind::ObjectStart {
        return Err(());
    }
    let mut result = Members { values: [None; 8] };
    let mut at = at.checked_add(1).ok_or(())?;
    for _ in 0..doc.len() {
        if kind(doc, at)? == Kind::ObjectEnd {
            for slot in 0..required.len() {
                result.at(slot)?;
            }
            return Ok(result);
        }
        let key = text(doc, at, Kind::Key)?;
        let mut slot = None;
        for (index, name) in required.iter().chain(optional).enumerate() {
            if *name == key {
                slot = Some(index);
                break;
            }
        }
        let entry = result.values.get_mut(slot.ok_or(())?).ok_or(())?;
        if entry.is_some() {
            return Err(());
        }
        let value = at.checked_add(1).ok_or(())?;
        *entry = Some(value);
        at = after(doc, value)?;
    }
    Err(())
}
fn array(doc: &Document, at: u32, cap: u32) -> Result<List<u32>, ()> {
    if kind(doc, at)? != Kind::ArrayStart {
        return Err(());
    }
    let mut entries = List::with_capacity(cap.min(doc.len()));
    let mut at = at.checked_add(1).ok_or(())?;
    for _ in 0..doc.len() {
        if kind(doc, at)? == Kind::ArrayEnd {
            return Ok(entries);
        }
        entries.push(at).or(Err(()))?;
        at = after(doc, at)?;
    }
    Err(())
}
fn root(doc: &Document) -> Result<(), ()> {
    if after(doc, 0)? == doc.len() { Ok(()) } else { Err(()) }
}
fn files(doc: &Document, at: u32, limits: &Limits) -> Result<Box<[api::File]>, ()> {
    let entries = array(doc, at, limits.documents.inner)?;
    let mut files = List::with_capacity(entries.len());
    for at in entries.into_boxed() {
        let member = object(doc, at, &[b"path", b"content"], &[])?;
        files
            .push(api::File {
                path: content(doc, member.at(0)?, limits.documents.body_bytes)?,
                content: content(doc, member.at(1)?, limits.documents.body_bytes)?,
            })
            .expect("sized files");
    }
    Ok(files.into_boxed())
}
pub(super) fn call(doc: &Document, name: Name, limits: &Limits) -> Result<Call, ()> {
    root(doc)?;
    let strings = limits.documents.string_bytes;
    match name {
        Name::Clone => {
            let m = object(doc, 0, &[b"repository"], &[])?;
            Ok(Call::Clone { repository: string(doc, m.at(0)?, strings)? })
        }
        Name::Fetch => {
            let m = object(doc, 0, &[b"repository"], &[b"branch", b"commit"])?;
            let want = match (m.find(1), m.find(2)) {
                (None, None) => api::Want::Default,
                (Some(at), None) => api::Want::Branch(string(doc, at, strings)?),
                (None, Some(at)) => api::Want::Commit(number(doc, at)?),
                (Some(_), Some(_)) => return Err(()),
            };
            Ok(Call::Fetch { repository: string(doc, m.at(0)?, strings)?, want })
        }
        Name::Create | Name::Push => {
            let optional: &[&[u8]] = if name == Name::Push { &[b"expected"] } else { &[] };
            let m = object(doc, 0, &[b"repository", b"branch", b"commit"], optional)?;
            let repository = string(doc, m.at(0)?, strings)?;
            let branch = string(doc, m.at(1)?, strings)?;
            let commit = number(doc, m.at(2)?)?;
            if name == Name::Create {
                Ok(Call::Create { repository, branch, commit })
            } else {
                Ok(Call::Push {
                    repository,
                    branch,
                    commit,
                    expected: match m.find(3) {
                        Some(at) => Some(number(doc, at)?),
                        None => None,
                    },
                })
            }
        }
        Name::Branch => {
            let m = object(doc, 0, &[b"repository", b"branch"], &[])?;
            Ok(Call::Branch { repository: string(doc, m.at(0)?, strings)?, branch: string(doc, m.at(1)?, strings)? })
        }
        Name::Objects => {
            let m = object(doc, 0, &[b"commits"], &[])?;
            let entries = array(doc, m.at(0)?, limits.documents.inner)?;
            let mut commits = List::with_capacity(entries.len());
            for at in entries.into_boxed() {
                commits.push(number(doc, at)?).expect("sized commits");
            }
            Ok(Call::Objects { commits: commits.into_boxed() })
        }
        Name::Commit | Name::Merge | Name::CommitTree => {
            let required: &[&[u8]] = if name == Name::Merge {
                &[b"parent", b"tree", b"message", b"merge_parent"]
            } else {
                &[b"parent", b"tree", b"message"]
            };
            let optional: &[&[u8]] = if name == Name::CommitTree { &[b"merging"] } else { &[] };
            let m = object(doc, 0, required, optional)?;
            let parent = number(doc, m.at(0)?)?;
            let tree = files(doc, m.at(1)?, limits)?;
            let message = content(doc, m.at(2)?, limits.documents.body_bytes)?;
            match name {
                Name::Commit => Ok(Call::Commit { parent, tree, message }),
                Name::Merge => Ok(Call::Merge { parent, tree, message, merge_parent: number(doc, m.at(3)?)? }),
                Name::CommitTree => Ok(Call::CommitTree {
                    parent,
                    tree,
                    message,
                    merging: match m.find(3) {
                        Some(at) => Some(number(doc, at)?),
                        None => None,
                    },
                }),
                Name::Clone
                | Name::Fetch
                | Name::Create
                | Name::Push
                | Name::Branch
                | Name::Objects
                | Name::Ancestor => unreachable!("store variant"),
            }
        }
        Name::Ancestor => {
            let m = object(doc, 0, &[b"ancestor", b"commit"], &[])?;
            Ok(Call::Ancestor { ancestor: number(doc, m.at(0)?)?, commit: number(doc, m.at(1)?)? })
        }
    }
}
fn parents(doc: &Document, m: &Members) -> Result<Parents, ()> {
    Ok(Parents {
        commit: number(doc, m.at(0)?)?,
        parent: optional(doc, m.at(1)?)?,
        merge_parent: optional(doc, m.at(2)?)?,
    })
}
pub(super) fn answer(doc: &Document, name: Name, limits: &Limits) -> Result<Answer, ()> {
    root(doc)?;
    match name {
        Name::Clone => {
            let m = object(doc, 0, &[b"default", b"branches"], &[])?;
            let default = string(doc, m.at(0)?, limits.documents.string_bytes)?;
            let entries = array(doc, m.at(1)?, limits.documents.inner)?;
            let mut branches = List::with_capacity(entries.len());
            for at in entries.into_boxed() {
                let m = object(doc, at, &[b"branch", b"commit"], &[])?;
                branches
                    .push(api::Head {
                        branch: string(doc, m.at(0)?, limits.documents.string_bytes)?,
                        commit: number(doc, m.at(1)?)?,
                    })
                    .expect("sized heads");
            }
            Ok(Answer::Cloned { default, branches: branches.into_boxed() })
        }
        Name::Fetch | Name::Branch => {
            let m = object(doc, 0, &[b"commit"], &[])?;
            Ok(Answer::Commit(number(doc, m.at(0)?)?))
        }
        Name::Create => {
            let m = object(doc, 0, &[b"result"], &[])?;
            Ok(Answer::Created(match text(doc, m.at(0)?, Kind::String)? {
                b"created" => api::Created::Created,
                b"exists" => api::Created::Exists,
                _ => return Err(()),
            }))
        }
        Name::Push => {
            let m = object(doc, 0, &[b"result"], &[])?;
            Ok(Answer::Pushed(match text(doc, m.at(0)?, Kind::String)? {
                b"pushed" => api::Pushed::Pushed,
                b"rejected" => api::Pushed::Rejected,
                _ => return Err(()),
            }))
        }
        Name::Commit | Name::Merge | Name::CommitTree => {
            let m = object(doc, 0, &[b"commit"], &[])?;
            Ok(Answer::Stored(optional(doc, m.at(0)?)?))
        }
        Name::Ancestor => {
            let m = object(doc, 0, &[b"ancestor"], &[])?;
            Ok(Answer::Ancestor(match kind(doc, m.at(0)?)? {
                Kind::True => true,
                Kind::False => false,
                Kind::ObjectStart
                | Kind::ObjectEnd
                | Kind::ArrayStart
                | Kind::ArrayEnd
                | Kind::Key
                | Kind::String
                | Kind::Number
                | Kind::Null
                | Kind::Long => return Err(()),
            }))
        }
        Name::Objects => {
            let m = object(doc, 0, &[b"objects", b"graph"], &[])?;
            let entries = array(doc, m.at(0)?, limits.documents.inner)?;
            let mut objects = List::with_capacity(entries.len());
            for at in entries.into_boxed() {
                let m = object(doc, at, &[b"commit", b"parent", b"merge_parent", b"tree", b"message"], &[])?;
                let p = parents(doc, &m)?;
                objects
                    .push(Object {
                        commit: p.commit,
                        parent: p.parent,
                        merge_parent: p.merge_parent,
                        tree: files(doc, m.at(3)?, limits)?,
                        message: content(doc, m.at(4)?, limits.documents.body_bytes)?,
                    })
                    .expect("sized objects");
            }
            let entries = array(doc, m.at(1)?, limits.documents.inner.saturating_sub(objects.len()))?;
            let mut graph = List::with_capacity(entries.len());
            for at in entries.into_boxed() {
                let m = object(doc, at, &[b"commit", b"parent", b"merge_parent"], &[])?;
                graph.push(parents(doc, &m)?).expect("sized graph");
            }
            Ok(Answer::Objects { objects: objects.into_boxed(), graph: graph.into_boxed() })
        }
    }
}
pub(super) fn error(doc: &Document) -> Result<api::Error, ()> {
    use api::Error as E;
    root(doc)?;
    let m = object(doc, 0, &[b"error"], &[b"what", b"reset", b"number", b"head", b"position"])?;
    let name = text(doc, m.at(0)?, Kind::String)?;
    let (error, fields): (E, &[&[u8]]) = match name {
        b"missing" => {
            let value = text(doc, m.at(1)?, Kind::String)?;
            let mut what = None;
            for candidate in [
                api::What::Repository,
                api::What::Item,
                api::What::Pull,
                api::What::Comment,
                api::What::Label,
                api::What::Branch,
                api::What::Commit,
                api::What::File,
                api::What::Page,
                api::What::Review,
                api::What::Job,
            ] {
                if super::write::what_name(candidate) == value {
                    what = Some(candidate);
                }
            }
            (E::Missing(what.ok_or(())?), &[b"error", b"what"])
        }
        b"rate_limited" => (E::RateLimited { reset: Time::from_nanos(number(doc, m.at(2)?)?) }, &[b"error", b"reset"]),
        b"already_queued" => (
            E::AlreadyQueued(api::QueueEntry {
                number: number(doc, m.at(3)?)?,
                head: number(doc, m.at(4)?)?,
                position: u32::try_from(number(doc, m.at(5)?)?).or(Err(()))?,
            }),
            &[b"error", b"number", b"head", b"position"],
        ),
        b"unavailable" => (E::Unavailable, &[b"error"]),
        b"timeout" => (E::Timeout, &[b"error"]),
        b"forbidden" => (E::Forbidden, &[b"error"]),
        b"too_large" => (E::TooLarge, &[b"error"]),
        b"full" => (E::Full, &[b"error"]),
        b"exists" => (E::Exists, &[b"error"]),
        b"circular" => (E::Circular, &[b"error"]),
        b"nothing_to_merge" => (E::NothingToMerge, &[b"error"]),
        b"empty" => (E::Empty, &[b"error"]),
        b"closed" => (E::Closed, &[b"error"]),
        b"stale" => (E::Stale, &[b"error"]),
        b"conflict" => (E::Conflict, &[b"error"]),
        b"protected" => (E::Protected, &[b"error"]),
        b"unreachable" => (E::Unreachable, &[b"error"]),
        b"refused" => (E::Refused, &[b"error"]),
        _ => return Err(()),
    };
    object(doc, 0, fields, &[])?;
    Ok(error)
}
