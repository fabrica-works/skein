use super::{Answer, Call, Object, Parents};
use crate::Limits;
use alloc::boxed::Box;
use skein_fake_forge_domain::api;
use skein_json::writer::{self, Encoder};
use skein_lib::{Writer, base64};

pub(super) fn finish(measure: Encoder, limits: &Limits) -> Result<Encoder, writer::Refusal> {
    let length = measure.measured()?;
    if length > limits.documents.body_bytes.min(limits.documents.json.length) {
        return Err(writer::Refusal::TooLong);
    }
    Ok(Encoder::write(length, &limits.documents.writer))
}
fn text(json: &mut Encoder, key: &[u8], value: &[u8]) {
    json.key(key);
    json.string(value);
}
fn number(json: &mut Encoder, key: &[u8], value: u64) {
    json.key(key);
    json.unsigned(value);
}
fn optional(json: &mut Encoder, key: &[u8], value: Option<u64>) {
    json.key(key);
    if let Some(value) = value {
        json.unsigned(value);
    } else {
        json.null();
    }
}
fn content(json: &mut Encoder, key: &[u8], value: &[u8]) -> Result<(), writer::Refusal> {
    let count = u32::try_from(value.len()).or(Err(writer::Refusal::TooLong))?;
    let length = base64::encoded_len(base64::Alphabet::Standard, count).ok_or(writer::Refusal::TooLong)?;
    let mut encoded = Writer::new(usize::try_from(length).expect("u32 fits"));
    base64::encode(base64::Alphabet::Standard, value, &mut encoded).or(Err(writer::Refusal::TooLong))?;
    text(json, key, &encoded.finish());
    Ok(())
}
fn tree(json: &mut Encoder, files: &[api::File]) -> Result<(), writer::Refusal> {
    json.key(b"tree");
    json.array_start();
    for file in files {
        json.object_start();
        content(json, b"path", &file.path)?;
        content(json, b"content", &file.content)?;
        json.object_end();
    }
    json.array_end();
    Ok(())
}
pub(super) fn call(json: &mut Encoder, call: &Call) -> Result<(), writer::Refusal> {
    json.object_start();
    match call {
        Call::Clone { repository } => text(json, b"repository", repository),
        Call::Fetch { repository, want } => {
            text(json, b"repository", repository);
            match want {
                api::Want::Branch(branch) => text(json, b"branch", branch),
                api::Want::Commit(commit) => number(json, b"commit", *commit),
                api::Want::Default => {}
            }
        }
        Call::Create { repository, branch, commit } => {
            text(json, b"repository", repository);
            text(json, b"branch", branch);
            number(json, b"commit", *commit);
        }
        Call::Push { repository, branch, commit, expected } => {
            text(json, b"repository", repository);
            text(json, b"branch", branch);
            number(json, b"commit", *commit);
            if let Some(expected) = expected {
                number(json, b"expected", *expected);
            }
        }
        Call::Branch { repository, branch } => {
            text(json, b"repository", repository);
            text(json, b"branch", branch);
        }
        Call::Objects { commits } => {
            json.key(b"commits");
            json.array_start();
            for commit in commits {
                json.unsigned(*commit);
            }
            json.array_end();
        }
        Call::Commit { parent, tree: files, message } => {
            number(json, b"parent", *parent);
            tree(json, files)?;
            content(json, b"message", message)?;
        }
        Call::Merge { parent, merge_parent, tree: files, message } => {
            number(json, b"parent", *parent);
            number(json, b"merge_parent", *merge_parent);
            tree(json, files)?;
            content(json, b"message", message)?;
        }
        Call::CommitTree { parent, merging, tree: files, message } => {
            number(json, b"parent", *parent);
            if let Some(merging) = merging {
                number(json, b"merging", *merging);
            }
            tree(json, files)?;
            content(json, b"message", message)?;
        }
        Call::Ancestor { ancestor, commit } => {
            number(json, b"ancestor", *ancestor);
            number(json, b"commit", *commit);
        }
    }
    json.object_end();
    Ok(())
}
pub(super) fn parents(json: &mut Encoder, value: Parents) {
    number(json, b"commit", value.commit);
    optional(json, b"parent", value.parent);
    optional(json, b"merge_parent", value.merge_parent);
}
pub(super) fn object(json: &mut Encoder, value: &Object) -> Result<(), writer::Refusal> {
    json.object_start();
    parents(json, Parents { commit: value.commit, parent: value.parent, merge_parent: value.merge_parent });
    tree(json, &value.tree)?;
    content(json, b"message", &value.message)?;
    json.object_end();
    Ok(())
}
pub(super) fn answer(json: &mut Encoder, answer: &Answer) -> Result<(), writer::Refusal> {
    json.object_start();
    match answer {
        Answer::Cloned { default, branches } => {
            text(json, b"default", default);
            json.key(b"branches");
            json.array_start();
            for head in branches {
                json.object_start();
                text(json, b"branch", &head.branch);
                number(json, b"commit", head.commit);
                json.object_end();
            }
            json.array_end();
        }
        Answer::Commit(commit) => number(json, b"commit", *commit),
        Answer::Stored(commit) => optional(json, b"commit", *commit),
        Answer::Created(created) => text(
            json,
            b"result",
            match created {
                api::Created::Created => b"created",
                api::Created::Exists => b"exists",
            },
        ),
        Answer::Pushed(pushed) => text(
            json,
            b"result",
            match pushed {
                api::Pushed::Pushed => b"pushed",
                api::Pushed::Rejected => b"rejected",
            },
        ),
        Answer::Objects { objects, graph } => {
            json.key(b"objects");
            json.array_start();
            for value in objects {
                object(json, value)?;
            }
            json.array_end();
            json.key(b"graph");
            json.array_start();
            for value in graph {
                json.object_start();
                parents(json, *value);
                json.object_end();
            }
            json.array_end();
        }
        Answer::Ancestor(value) => {
            json.key(b"ancestor");
            json.boolean(*value);
        }
    }
    json.object_end();
    Ok(())
}
pub(super) fn error(json: &mut Encoder, error: api::Error) {
    use api::Error as E;
    let name: &[u8] = match error {
        E::Unavailable => b"unavailable",
        E::Timeout => b"timeout",
        E::RateLimited { .. } => b"rate_limited",
        E::Forbidden => b"forbidden",
        E::Missing(_) => b"missing",
        E::TooLarge => b"too_large",
        E::Full => b"full",
        E::Exists => b"exists",
        E::AlreadyQueued(_) => b"already_queued",
        E::Circular => b"circular",
        E::NothingToMerge => b"nothing_to_merge",
        E::Empty => b"empty",
        E::Closed => b"closed",
        E::Stale => b"stale",
        E::Conflict => b"conflict",
        E::Protected => b"protected",
        E::Unreachable => b"unreachable",
        E::Refused => b"refused",
    };
    json.object_start();
    text(json, b"error", name);
    match error {
        E::Missing(what) => text(json, b"what", what_name(what)),
        E::RateLimited { reset } => number(json, b"reset", reset.as_nanos()),
        E::AlreadyQueued(entry) => {
            number(json, b"number", entry.number);
            number(json, b"head", entry.head);
            number(json, b"position", u64::from(entry.position));
        }
        E::Unavailable
        | E::Timeout
        | E::Forbidden
        | E::TooLarge
        | E::Full
        | E::Exists
        | E::Circular
        | E::NothingToMerge
        | E::Empty
        | E::Closed
        | E::Stale
        | E::Conflict
        | E::Protected
        | E::Unreachable
        | E::Refused => {}
    }
    json.object_end();
}
pub(super) fn what_name(what: api::What) -> &'static [u8] {
    match what {
        api::What::Repository => b"repository",
        api::What::Item => b"item",
        api::What::Pull => b"pull",
        api::What::Comment => b"comment",
        api::What::Label => b"label",
        api::What::Branch => b"branch",
        api::What::Commit => b"commit",
        api::What::File => b"file",
        api::What::Page => b"page",
        api::What::Review => b"review",
        api::What::Job => b"job",
    }
}
pub(super) fn encode_error(error: api::Error, limits: &Limits) -> Result<Box<[u8]>, writer::Refusal> {
    let mut measure = Encoder::measure(&limits.documents.writer);
    self::error(&mut measure, error);
    let mut write = finish(measure, limits)?;
    self::error(&mut write, error);
    Ok(write.finish())
}
