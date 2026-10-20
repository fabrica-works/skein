//! Git's direct store calls borrow the forge only for this pass.
use crate::{
    Hosted, Limits,
    git::{self, Answer, Call, Parents},
};
use alloc::boxed::Box;
use core::cmp::Reverse;
use skein_fake_forge_domain::{self as domain, api};
use skein_json::writer::Encoder;
use skein_lib::{List, Set};

pub(crate) fn answer(hosted: &mut Hosted<'_>, call: Call, limits: &Limits, cap: u32) -> Result<Box<[u8]>, api::Error> {
    let answer = match call {
        Call::Commit { parent, tree, message } => {
            Answer::Stored(domain::commit(hosted.domain, hosted.config, parent, tree, &message)?)
        }
        Call::Merge { parent, merge_parent, tree, message } => Answer::Stored(Some(domain::merge_commit(
            hosted.domain,
            hosted.config,
            parent,
            merge_parent,
            tree,
            &message,
        )?)),
        Call::CommitTree { parent, merging, tree, message } => {
            Answer::Stored(Some(domain::commit_tree(hosted.domain, hosted.config, parent, merging, tree, &message)?))
        }
        Call::Ancestor { ancestor, commit } => {
            if hosted.domain.object(ancestor).is_none() || hosted.domain.object(commit).is_none() {
                return Err(api::Error::Missing(api::What::Commit));
            }
            Answer::Ancestor(hosted.domain.is_ancestor(ancestor, commit))
        }
        Call::Objects { commits } => return objects(hosted.domain, &commits, limits, cap),
        Call::Clone { .. } | Call::Fetch { .. } | Call::Create { .. } | Call::Push { .. } | Call::Branch { .. } => {
            unreachable!("only store calls borrow the object database")
        }
    };
    git::server::encode(&answer, limits).or(Err(api::Error::TooLarge))
}

fn add_parents(frontier: &mut Set<Reverse<u64>>, object: &domain::Object) {
    for parent in [object.parent, object.merge_parent].into_iter().flatten() {
        frontier.insert(Reverse(parent)).expect("two parents per given entry");
    }
}
fn parents(commit: u64, object: &domain::Object) -> Parents {
    Parents { commit, parent: object.parent, merge_parent: object.merge_parent }
}
fn object(json: &mut Encoder, commit: u64, object: &domain::Object) -> Result<(), api::Error> {
    json.object_start();
    git::write::parents(json, parents(commit, object));
    json.key(b"tree");
    json.array_start();
    for (path, content) in &object.tree {
        json.object_start();
        git::write::content(json, b"path", path).or(Err(api::Error::TooLarge))?;
        git::write::content(json, b"content", content).or(Err(api::Error::TooLarge))?;
        json.object_end();
    }
    json.array_end();
    git::write::content(json, b"message", &object.message).or(Err(api::Error::TooLarge))?;
    json.object_end();
    Ok(())
}
fn objects(domain: &domain::Domain, asked: &[u64], limits: &Limits, cap: u32) -> Result<Box<[u8]>, api::Error> {
    if asked.len() > usize::try_from(cap).expect("u32 fits") {
        return Err(api::Error::Refused);
    }
    for commit in asked {
        if domain.object(*commit).is_none() {
            return Err(api::Error::Missing(api::What::Commit));
        }
    }
    let mut whole = List::with_capacity(cap);
    let mut graph = List::with_capacity(cap);
    // At most cap input entries and cap output entries can be expanded;
    // each adds at most two parents. Counts, never trees, inhabit this set.
    let mut frontier = Set::with_capacity(cap.checked_mul(4).expect("validated walk bound"));
    let room = limits.documents.writer.length.min(limits.documents.body_bytes).min(limits.documents.json.length);
    let mut length = u32::try_from(b"{\"objects\":[],\"graph\":[]}".len()).expect("fixed envelope");
    if length > room {
        return Err(api::Error::TooLarge);
    }
    for commit in asked {
        let value = domain.object(*commit).expect("asked objects checked");
        let mut measure = Encoder::measure(&limits.documents.writer);
        object(&mut measure, *commit, value)?;
        let size = measure.measured().or(Err(api::Error::TooLarge))?;
        let next = length
            .checked_add(size)
            .ok_or(api::Error::TooLarge)?
            .checked_add(u32::from(!whole.is_empty()))
            .ok_or(api::Error::TooLarge)?;
        if next > room {
            break;
        }
        length = next;
        whole.push(*commit).expect("asked bound");
        add_parents(&mut frontier, value);
    }
    if !asked.is_empty() && whole.is_empty() {
        return Err(api::Error::TooLarge);
    }
    for _ in 0..cap.saturating_mul(2) {
        if whole.len().saturating_add(graph.len()) >= cap {
            break;
        }
        let Some(Reverse(commit)) = frontier.pop_first() else { break };
        let value = domain.object(commit).expect("parents are stored");
        if asked.contains(&commit) {
            // Selected asked nodes already seeded their parents. An asked
            // node omitted for room still connects the returned history.
            if !whole.as_slice().contains(&commit) {
                add_parents(&mut frontier, value);
            }
            continue;
        }
        let row = parents(commit, value);
        let mut measure = Encoder::measure(&limits.documents.writer);
        measure.object_start();
        git::write::parents(&mut measure, row);
        measure.object_end();
        let size = measure.measured().or(Err(api::Error::TooLarge))?;
        let next = length
            .checked_add(size)
            .ok_or(api::Error::TooLarge)?
            .checked_add(u32::from(!graph.is_empty()))
            .ok_or(api::Error::TooLarge)?;
        if next > room {
            break;
        }
        length = next;
        graph.push(row).expect("response item bound");
        add_parents(&mut frontier, value);
    }
    let mut json = Encoder::write(length, &limits.documents.writer);
    json.object_start();
    json.key(b"objects");
    json.array_start();
    for commit in &whole {
        object(&mut json, *commit, domain.object(*commit).expect("selected object"))?;
    }
    json.array_end();
    json.key(b"graph");
    json.array_start();
    for row in &graph {
        json.object_start();
        git::write::parents(&mut json, *row);
        json.object_end();
    }
    json.array_end();
    json.object_end();
    Ok(json.finish())
}
