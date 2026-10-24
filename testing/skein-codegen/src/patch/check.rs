//! The read-only findings of codec.md 12.7.

use alloc::collections::{BTreeMap, BTreeSet};
use skein_codec::patch::{Capacity, KeepError, PatchError, PatchWrong, Patcher};
use skein_codec::schema;
use std::path::Path;

use super::files::{Crate, Text};
use super::run::{historical, read_optional};
use super::text::{Block, PatchText};
use super::{Error, Finding, Landed, derive};
use crate::diff::Version;

/// Finds every change or refusal without writing a file (codec.md 12.7); a
/// crate it cannot read is its error, as the run's.
pub fn check(owner: &Path, landed: &Landed) -> Result<Vec<Finding>, Error> {
    let tree = Crate::read(owner)?;
    let base = Crate::base(owner, landed)?;
    let patches = derive::patches(&tree)?;
    let _branch = derive::branch(&tree, &base)?;
    let frozen = derive::frozen(&derive::patches(&base)?);
    let mut findings = Vec::new();
    for (family, current) in &tree.current {
        let Some(texts) = tree.kept.get(family) else {
            findings.push(Finding::Unkept { family: family.clone() });
            continue;
        };
        current_findings(&tree, family, current, texts, &patches, &frozen, &mut findings)?;
    }
    for (family, texts) in &tree.kept {
        if let Some((first, _)) = texts.first_key_value()
            && let Some((last, _)) = texts.last_key_value()
        {
            for version in *first..=*last {
                if !texts.contains_key(&version) {
                    findings.push(Finding::KeptGap { family: family.clone(), missing: version });
                } else if version != *first && !moves_to(&patches, family, version) {
                    findings.push(Finding::Unpatched { family: family.clone(), version });
                }
            }
        }
    }
    for (path, bytes) in &landed.files {
        if historical(path) && read_optional(&owner.join(path))?.as_deref() != Some(bytes.as_slice()) {
            let refreshed = tree
                .kept
                .values()
                .filter_map(|texts| texts.last_key_value().map(|(_, text)| text))
                .find(|text| text.path == *path)
                .is_some_and(|text| {
                    !frozen.contains(&(text.family.clone(), text.version))
                        && tree.current.get(&text.family).is_some_and(|current| current.source == text.source)
                });
            if !refreshed {
                findings.push(Finding::Changed { path: path.clone() });
            }
        }
    }
    if let Some(last) = patches.keys().next_back() {
        for number in 1..=*last {
            if !patches.contains_key(&number) {
                findings.push(Finding::PatchGap { missing: number });
            }
        }
    }
    for (number, patch) in &patches {
        if patch.level != *number {
            findings.push(Finding::Misnumbered { patch: *number, level: patch.level });
        }
        for block in &patch.blocks {
            if let Err(error) = planned(&tree, *number, block) {
                findings.push(Finding::Unplanned { patch: *number, family: block.family.clone(), error });
            }
        }
    }
    Ok(findings)
}

fn moves_to(patches: &BTreeMap<u32, PatchText>, family: &str, version: u16) -> bool {
    patches.values().flat_map(|patch| &patch.blocks).any(|block| block.family == family && block.to == version)
}

fn current_findings(
    tree: &Crate,
    family: &str,
    current: &Text,
    texts: &BTreeMap<u16, Text>,
    patches: &BTreeMap<u32, PatchText>,
    frozen: &BTreeSet<(String, u16)>,
    findings: &mut Vec<Finding>,
) -> Result<(), Error> {
    let Some((_, kept)) = texts.last_key_value() else { return Ok(()) };
    let blocks: Vec<_> = patches
        .values()
        .flat_map(|patch| &patch.blocks)
        .filter(|block| block.family == family && block.from == kept.version)
        .collect();
    let renames = derive::confirmed(tree, family, &blocks, &[]);
    let diff = derive::compare(kept, current, &renames)?;
    let frozen = frozen.contains(&(family.to_owned(), kept.version));
    let verdict = derive::verdict(&diff, frozen);
    if derive::needs(&diff, frozen) && !moves_to(patches, family, current.version) {
        findings.push(Finding::Unpatched { family: family.into(), version: current.version });
    }
    match verdict {
        Version::Kept => {
            if !frozen && kept.source != current.source {
                findings.push(Finding::Lagging { family: family.into(), version: kept.version });
            }
        }
        Version::Moved => {}
        Version::Unmoved | Version::Needless | Version::Wrong => {
            findings.push(Finding::Version {
                family: family.into(),
                verdict,
                kept: kept.version,
                current: current.version,
            });
        }
    }
    Ok(())
}

pub(super) fn planned(tree: &Crate, number: u32, block: &Block) -> Result<(), String> {
    let source = tree.patches.get(&number).expect("parsed patch has source");
    let texts = tree.kept.get(&block.family);
    let old = texts.and_then(|texts| texts.get(&block.from));
    let new = texts.and_then(|texts| texts.get(&block.to));
    let length = old.into_iter().chain(new).map(|text| text.source.len()).max().unwrap_or(0).max(source.len());
    let text = u32::try_from(length)
        .ok()
        .filter(|length| *length <= schema::MAX_TEXT)
        .ok_or_else(|| "line 1: Long".to_owned())?;
    // Planning allocates schema and plan tables only; no record is decoded.
    let mut patcher = Patcher::new(Capacity { versions: 2, patches: 1, text, bytes: 0, nodes: 0 });
    for text in old.into_iter().chain(new) {
        if patcher.schema(text.version).is_none() {
            patcher.keep(text.source.as_bytes()).map_err(|error| match error {
                KeepError::Schema(error) => format!("schema at line {} refused", error.line),
                KeepError::Family => "kept text: Family".into(),
                KeepError::Twice => "kept text: Twice".into(),
                KeepError::Room => "kept text: Room".into(),
            })?;
        }
    }
    patcher.plan(source.as_bytes()).map_err(spell)?.ok_or_else(|| {
        let line = source
            .lines()
            .position(|line| {
                let mut words = line.split_whitespace();
                words.next() == Some("family") && words.next() == Some(block.family.as_str())
            })
            .unwrap_or(0)
            .checked_add(1)
            .expect("bounded patch lines");
        format!("line {line}: Unkept")
    })?;
    Ok(())
}

fn spell(error: PatchError) -> String {
    let name = match error.wrong {
        PatchWrong::Long => "Long",
        PatchWrong::Character => "Character",
        PatchWrong::Expected(_) => "Expected",
        PatchWrong::Number => "Number",
        PatchWrong::Language => "Language",
        PatchWrong::Twice => "Twice",
        PatchWrong::Unkept => "Unkept",
        PatchWrong::Version => "Version",
        PatchWrong::Unknown => "Unknown",
        PatchWrong::Mismatch => "Mismatch",
        PatchWrong::Missing(_) => "Missing",
        PatchWrong::Asked => "Asked",
        PatchWrong::Value => "Value",
        PatchWrong::Site => "Site",
        PatchWrong::Stray => "Stray",
        PatchWrong::Pieces => "Pieces",
        PatchWrong::Room => "Room",
    };
    format!("line {} at {}..{}: {name}", error.line, error.at.start, error.at.end)
}
