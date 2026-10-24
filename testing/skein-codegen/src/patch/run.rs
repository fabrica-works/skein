//! Derive the branch's patch, then apply the staged file changes.

use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use skein_codec::patch::{Build, Halt, Old, Site};
use skein_codec::value::Node;

use super::files::{self, Crate, kept_path, patch_path, renames_path, skeleton_path};
use super::text::{Block, Line, PatchText};
use super::{Error, Landed, Options, Outcome, Report, check, derive, golden, module, renumber, skeleton};
use crate::diff::{Rename, Version};

pub(super) type Writes = BTreeMap<PathBuf, Option<Vec<u8>>>;

/// Runs codec.md 12.2 over an owner's crate, with the base supplied by its caller.
pub fn run(owner: &Path, landed: &Landed, options: &Options) -> Result<Report, Error> {
    let tree = Crate::read(owner)?;
    let base = Crate::base(owner, landed)?;
    if tree.kept.is_empty() && base.kept.is_empty() && !options.baseline {
        return Err(Error::Baseline);
    }
    let patches = derive::patches(&tree)?;
    let landed_patches = derive::patches(&base)?;
    let frozen = derive::frozen(&landed_patches);
    let own = derive::branch(&tree, &base)?;
    let old = own.and_then(|number| patches.get(&number));
    let number = base.patches.keys().next_back().copied().unwrap_or(0).checked_add(1).ok_or_else(|| Error::Schema {
        path: PathBuf::from(files::PATCHES),
        error: "patch number exceeds u32".into(),
    })?;
    let Families { mut writes, mut report, blocks, refreshed } = families(&tree, &base, old, options, &frozen)?;
    if blocks.is_empty() {
        if let Some(own) = own {
            writes.insert(patch_path(own), None);
            remove_goldens(owner, own, &mut writes)?;
            if tree.skeletons.contains(&own) {
                report.asked.push(format!("remove {}: patch has no block", skeleton_path(own).display()));
            }
        }
    } else {
        let owner_name = options
            .owner
            .as_ref()
            .or_else(|| old.map(|patch| &patch.owner))
            .or_else(|| landed_patches.values().next_back().map(|patch| &patch.owner))
            .filter(|owner| !owner.is_empty())
            .ok_or(Error::Owner)?
            .clone();
        let patch = PatchText {
            owner: owner_name,
            level: number,
            after: if options.after.is_empty() {
                old.map(|patch| patch.after.clone()).unwrap_or_default()
            } else {
                options.after.clone()
            },
            marks: old.map(|patch| patch.marks.clone()).unwrap_or_default(),
            blocks,
        };
        let source = if old == Some(&patch) {
            own.and_then(|number| tree.patches.get(&number)).cloned().expect("old patch has its source")
        } else {
            derive::render(&patch)
        };
        writes.insert(patch_path(number), Some(source.into_bytes()));
        report.patch = Some(number);
        asks(number, &patch, &mut report);
        if let Some(own) = own
            && own != number
        {
            writes.insert(patch_path(own), None);
            renumber::move_files(
                owner,
                landed,
                own,
                number,
                old.expect("branch patch parsed"),
                &patch,
                &mut writes,
                &mut report,
            )?;
            report.moved = Some(own);
        }
    }
    restore(owner, landed, &refreshed, &mut writes, &mut report)?;
    report.restored.sort();
    report.restored.dedup();
    apply(owner, &writes)?;
    generated(owner, report.patch)?;
    report.findings = check(owner, landed)?;
    Ok(report)
}

/// The files beside the patch's text (12.2, 7; 12.3): its skeleton, a
/// function appended for each site it lacks; `context.rs` once; `mod.rs`
/// again; and, for a patch with no site once it plans, its goldens.
fn generated(owner: &Path, branch: Option<u32>) -> Result<(), Error> {
    let mut krate = Crate::read(owner)?;
    let mut writes = Writes::new();
    let text = branch.and_then(|number| krate.patches.get(&number).map(|source| (number, source)));
    let text =
        text.map(|(number, source)| module::read_patch(source, number).map(|text| (number, text))).transpose()?;
    if let Some((number, text)) = &text
        && text.sites().next().is_some()
    {
        let path = skeleton_path(*number);
        let existing = read_optional(&owner.join(&path))?.map(String::from_utf8).transpose();
        let existing = existing.map_err(|error| Error::Schema { path: path.clone(), error: error.to_string() })?;
        writes.insert(path, Some(skeleton::skeleton_text(*number, text, existing.as_deref())?.into_bytes()));
        krate.skeletons.insert(*number);
    }
    let context = Path::new(files::PATCHES).join("context.rs");
    if read_optional(&owner.join(&context))?.is_none() {
        writes.insert(context, Some(skeleton::CONTEXT.as_bytes().to_vec()));
    }
    writes.insert(Path::new(files::PATCHES).join("mod.rs"), Some(module::module_text(&krate)?.into_bytes()));
    apply(owner, &writes)?;
    if let Some((number, text)) = &text
        && text.sites().next().is_none()
        && text.blocks.iter().all(|block| check::planned(&krate, *number, block).is_ok())
    {
        golden::run_goldens::<()>(&krate, *number, unwritten, true)?;
    }
    Ok(())
}

/// The dispatch of a patch with no site, which no run calls.
fn unwritten(site: Site, _old: &Old, _new: &mut Build, _at: Node, _cx: &mut ()) -> Result<(), Halt> {
    Err(Halt::NoSite(site))
}

struct Families {
    writes: Writes,
    report: Report,
    blocks: Vec<Block>,
    refreshed: BTreeSet<PathBuf>,
}

fn families(
    tree: &Crate,
    base: &Crate,
    old: Option<&PatchText>,
    options: &Options,
    frozen: &BTreeSet<(String, u16)>,
) -> Result<Families, Error> {
    let mut writes = Writes::new();
    let mut report = Report::default();
    let mut blocks = Vec::new();
    let mut refreshed = BTreeSet::new();
    for (family, current) in &tree.current {
        let Some((_, kept)) = base.kept.get(family).and_then(|texts| texts.last_key_value()) else {
            remove_other_kept(tree, family, current.version, &mut writes);
            writes.insert(kept_path(family, current.version), Some(current.source.as_bytes().to_vec()));
            writes.insert(renames_path(family), None);
            report.outcomes.push(Outcome::Kept { family: family.clone(), version: current.version });
            continue;
        };
        let previous = old.and_then(|patch| patch.blocks.iter().find(|block| block.family == *family));
        let is_frozen = frozen.contains(&(family.clone(), kept.version));
        let mut confirmed =
            derive::confirmed(tree, family, &previous.into_iter().collect::<Vec<_>>(), &options.renames);
        if !is_frozen {
            for rename in derive::refreshed_renames(kept, tree)? {
                if !confirmed.contains(&rename) {
                    let mut candidate = confirmed.clone();
                    candidate.push(rename);
                    if derive::compare(kept, current, &candidate).is_ok() {
                        confirmed = candidate;
                    }
                }
            }
        }
        let diff = derive::compare(kept, current, &confirmed)?;
        report.proposed.extend(
            derive::proposals(kept, current, &confirmed, &diff)?.into_iter().map(|rename| (family.clone(), rename)),
        );
        match derive::verdict(&diff, is_frozen) {
            Version::Kept => {
                if is_frozen {
                    waiting(family, &confirmed, &mut writes, &mut report);
                } else {
                    let path = kept_path(family, kept.version);
                    let same = read_optional(&tree.root.join(&path))?.as_deref() == Some(current.source.as_bytes());
                    writes.insert(path.clone(), Some(current.source.as_bytes().to_vec()));
                    writes.insert(renames_path(family), None);
                    refreshed.insert(path);
                    report.outcomes.push(if same {
                        Outcome::Current { family: family.clone() }
                    } else {
                        Outcome::Refreshed { family: family.clone(), version: kept.version }
                    });
                }
                remove_branch_kept(tree, base, family, kept.version, &mut writes);
            }
            Version::Moved => {
                blocks.push(derive::block(&diff, current, previous)?);
                remove_branch_kept(tree, base, family, current.version, &mut writes);
                writes.insert(kept_path(family, current.version), Some(current.source.as_bytes().to_vec()));
                writes.insert(renames_path(family), None);
                report.outcomes.push(Outcome::Patched {
                    family: family.clone(),
                    from: kept.version,
                    to: current.version,
                });
            }
            verdict @ (Version::Unmoved | Version::Needless | Version::Wrong) => {
                // Refused families retain their branch block and kept files.
                if let Some(previous) = previous {
                    blocks.push(previous.clone());
                }
                report.outcomes.push(Outcome::Refused { family: family.clone(), verdict });
            }
        }
    }
    Ok(Families { writes, report, blocks, refreshed })
}

fn waiting(family: &str, renames: &[Rename], writes: &mut Writes, report: &mut Report) {
    if renames.is_empty() {
        report.outcomes.push(Outcome::Current { family: family.into() });
    } else {
        let mut text = String::new();
        for rename in renames {
            writeln!(text, "{rename}").expect("String accepts formatting");
        }
        writes.insert(renames_path(family), Some(text.into_bytes()));
        report.outcomes.push(Outcome::Waiting { family: family.into(), renames: renames.to_vec() });
    }
}

fn remove_other_kept(tree: &Crate, family: &str, keep: u16, writes: &mut Writes) {
    if let Some(texts) = tree.kept.get(family) {
        for (version, text) in texts {
            if *version != keep {
                writes.insert(text.path.clone(), None);
            }
        }
    }
}

fn remove_branch_kept(tree: &Crate, base: &Crate, family: &str, keep: u16, writes: &mut Writes) {
    if let Some(texts) = tree.kept.get(family) {
        for (version, text) in texts {
            if *version != keep && !base.kept.get(family).is_some_and(|texts| texts.contains_key(version)) {
                writes.insert(text.path.clone(), None);
            }
        }
    }
}

fn asks(number: u32, patch: &PatchText, report: &mut Report) {
    for block in &patch.blocks {
        for line in &block.lines {
            if let Line::Op(line) = line
                && line.ends_with(" ?")
            {
                report.asked.push(format!("patch {number:04}, {}: {line}", block.family));
            }
        }
    }
    for (block, target, _) in patch.sites() {
        report.asked.push(format!("patch {number:04}, {}: written {target}", block.family));
    }
}

pub(super) fn remove_goldens(owner: &Path, number: u32, writes: &mut Writes) -> Result<(), Error> {
    for path in files::landed(owner)?.keys().filter(|path| path.starts_with(files::golden_path(number))) {
        writes.insert(path.clone(), None);
    }
    Ok(())
}

/// Whether a landed file belongs to the command's immutable history.
pub(super) fn historical(path: &Path) -> bool {
    (path.parent() == Some(Path::new(files::PATCHES)) && path.extension().is_some_and(|ext| ext == "patch"))
        || path.starts_with(Path::new(files::PATCHES).join("golden"))
        || (path.parent() == Some(Path::new(files::KEPT)) && path.extension().is_some_and(|ext| ext == "schema"))
}

fn restore(
    owner: &Path,
    landed: &Landed,
    refreshed: &BTreeSet<PathBuf>,
    writes: &mut Writes,
    report: &mut Report,
) -> Result<(), Error> {
    for (path, bytes) in &landed.files {
        if historical(path) && !refreshed.contains(path) {
            let held = read_optional(&owner.join(path))?;
            let staged = writes.get(path).map_or(held.as_deref(), |bytes| bytes.as_deref());
            if staged != Some(bytes.as_slice()) {
                writes.insert(path.clone(), Some(bytes.clone()));
                if held.as_deref() != Some(bytes.as_slice()) {
                    report.restored.push(path.clone());
                }
            }
        }
    }
    Ok(())
}

pub(super) fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io(path, &error)),
    }
}

fn apply(owner: &Path, writes: &Writes) -> Result<(), Error> {
    // Preflight every existing command-owned text, including the author's code.
    for (path, bytes) in files::landed(owner)? {
        if let Ok(text) = core::str::from_utf8(&bytes)
            && text.lines().any(|line| line.starts_with("<<<<<<< ") || line.starts_with(">>>>>>> "))
        {
            return Err(Error::Conflict { path });
        }
    }
    for (path, bytes) in writes {
        let full = owner.join(path);
        let held = read_optional(&full)?;
        if held == *bytes {
            continue;
        }
        if let Some(bytes) = bytes {
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|error| Error::io(parent, &error))?;
            }
            fs::write(&full, bytes).map_err(|error| Error::io(&full, &error))?;
        } else if held.is_some() {
            fs::remove_file(&full).map_err(|error| Error::io(&full, &error))?;
        }
    }
    // Empty golden directories carry no files and have no owner-authored content.
    for path in writes.keys().filter_map(|path| path.parent()) {
        if path.starts_with(Path::new(files::PATCHES).join("golden")) && owner.join(path).is_dir() {
            prune(owner, path)?;
        }
    }
    Ok(())
}

fn prune(owner: &Path, directory: &Path) -> Result<(), Error> {
    let full = owner.join(directory);
    if fs::read_dir(&full).map_err(|error| Error::io(&full, &error))?.next().is_none() {
        fs::remove_dir(&full).map_err(|error| Error::io(&full, &error))?;
        if let Some(parent) = directory.parent()
            && parent != Path::new(files::PATCHES)
        {
            prune(owner, parent)?;
        }
    }
    Ok(())
}
