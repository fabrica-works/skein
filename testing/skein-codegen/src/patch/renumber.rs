//! Move the author's files away from a number taken by the base.

use alloc::collections::{BTreeMap, BTreeSet};

use super::files::{golden_path, skeleton_path, unconflicted};
use super::run::{Writes, read_optional};
use super::text::PatchText;
use super::{Error, Landed, Report};
use std::path::Path;

#[expect(clippy::too_many_arguments, reason = "renumbering compares both numbers and both parsed patch texts")]
pub(super) fn move_files(
    owner: &Path,
    landed: &Landed,
    old: u32,
    new: u32,
    before: &PatchText,
    after: &PatchText,
    writes: &mut Writes,
    report: &mut Report,
) -> Result<(), Error> {
    for before in &before.blocks {
        if let Some(after) = after.blocks.iter().find(|block| block.family == before.family)
            && before.from != after.from
        {
            report.asked.push(format!(
                "review patches/p{new:04}.rs: {} now moves {} to {}",
                before.family, after.from, after.to
            ));
        }
    }
    let path = skeleton_path(old);
    if let Some(bytes) = read_optional(&owner.join(&path))?
        && landed.get(&path) != Some(bytes.as_slice())
    {
        let source = unconflicted(owner, &path)?;
        let source = source
            .replace(&format!("p{old:04}"), &format!("p{new:04}"))
            .replace(&format!("Site::new({old},"), &format!("Site::new({new},"));
        let source = shift_versions(&source, before, after);
        writes.insert(skeleton_path(new), Some(source.into_bytes()));
        writes.insert(path.clone(), landed.get(&path).map(<[u8]>::to_vec));
        if landed.get(&path).is_some() {
            report.restored.push(path);
        }
    }
    let files = super::files::landed(owner)?;
    for (path, bytes) in files.iter().filter(|(path, _)| path.starts_with(golden_path(old))) {
        if landed.get(path) == Some(bytes.as_slice()) {
            continue;
        }
        let relative = path.strip_prefix(golden_path(old)).expect("golden prefix filtered");
        let family = relative.components().next().and_then(|part| part.as_os_str().to_str()).unwrap_or_default();
        let moved = before
            .blocks
            .iter()
            .find(|block| block.family == family)
            .zip(after.blocks.iter().find(|block| block.family == family))
            .is_some_and(|(before, after)| before.from != after.from);
        if moved {
            report.asked.push(format!("{}: goldens dropped; {family}'s old kept version moved", path.display()));
        } else {
            writes.insert(golden_path(new).join(relative), Some(bytes.clone()));
        }
        writes.insert(path.clone(), None);
    }
    Ok(())
}

/// Rewrite version tokens together, so an adjacent version never shifts twice.
fn shift_versions(source: &str, before: &PatchText, after: &PatchText) -> String {
    let shifts: Vec<_> = before
        .blocks
        .iter()
        .filter_map(|old| {
            let new = after.blocks.iter().find(|block| block.family == old.family)?;
            (old.from != new.from).then(|| (old.family.replace('-', "_"), old.from, old.to, new.from, new.to))
        })
        .collect();
    let tokens = tokens(source);
    let mut replacements = BTreeMap::new();
    let mut imports = BTreeSet::new();
    let mut imported = BTreeMap::new();
    let mut in_use = false;
    for (index, (start, end)) in tokens.iter().copied().enumerate() {
        let word = source.get(start..end).expect("token span");
        if word == "use" {
            in_use = true;
        }
        if word == ";" {
            in_use = false;
        }
        let following =
            |offset: usize| tokens.get(index.checked_add(offset)?).and_then(|(start, end)| source.get(*start..*end));
        if following(1) != Some("::") {
            continue;
        }
        if following(2) == Some("{") {
            for (item_start, item_end) in tokens.iter().skip(index.checked_add(3).expect("token index")).copied() {
                let item = source.get(item_start..item_end).expect("token span");
                if item == "}" {
                    break;
                }
                if let Some(version) = version(item) {
                    if in_use {
                        imports.insert(word.to_owned());
                        imported.insert(item.to_owned(), word.to_owned());
                    }
                    if let Some(shift) = shifts.iter().find(|shift| shift.0 == word) {
                        replace_version(&mut replacements, item_start, item_end, version, shift);
                    }
                }
            }
        } else if let Some(version) = following(2).and_then(version) {
            let (item_start, item_end) =
                *tokens.get(index.checked_add(2).expect("token index")).expect("version token");
            if in_use {
                imports.insert(word.to_owned());
                imported.insert(format!("v{version}"), word.to_owned());
            }
            if let Some(shift) = shifts.iter().find(|shift| shift.0 == word) {
                replace_version(&mut replacements, item_start, item_end, version, shift);
            }
        }
    }
    if imports.len() == 1 {
        for (index, (start, end)) in tokens.iter().copied().enumerate() {
            let word = source.get(start..end).expect("token span");
            let next = tokens
                .get(index.checked_add(1).expect("token index"))
                .and_then(|(start, end)| source.get(*start..*end));
            let previous = index
                .checked_sub(1)
                .and_then(|index| tokens.get(index))
                .and_then(|(start, end)| source.get(*start..*end));
            if next == Some("::")
                && previous != Some("::")
                && let Some(family) = imported.get(word)
                && let Some(shift) = shifts.iter().find(|shift| &shift.0 == family)
                && let Some(version) = version(word)
            {
                replace_version(&mut replacements, start, end, version, shift);
            }
        }
    }
    let mut result = String::new();
    let mut from = 0;
    for ((start, end), replacement) in replacements {
        result.push_str(source.get(from..start).expect("ordered replacements"));
        result.push_str(&replacement);
        from = end;
    }
    result.push_str(source.get(from..).expect("remaining source"));
    result
}

fn version(word: &str) -> Option<u16> {
    word.strip_prefix('v')?.parse().ok()
}

fn replace_version(
    replacements: &mut BTreeMap<(usize, usize), String>,
    start: usize,
    end: usize,
    version: u16,
    shift: &(String, u16, u16, u16, u16),
) {
    let new = if version == shift.1 {
        shift.3
    } else if version == shift.2 {
        shift.4
    } else {
        return;
    };
    replacements.insert((start, end), format!("v{new}"));
}

/// Rust words and punctuation outside comments and string literals; spans
/// keep every byte of the author's spacing and comments during rewriting.
fn tokens(source: &str) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut at = 0;
    while let Some(rest) = source.get(at..) {
        let Some(character) = rest.chars().next() else { break };
        let length = if rest.starts_with("//") {
            rest.find('\n').unwrap_or(rest.len())
        } else if rest.starts_with("/*") {
            rest.find("*/").and_then(|end| end.checked_add(2)).unwrap_or(rest.len())
        } else if character == '"' {
            let mut escaped = false;
            let mut length = character.len_utf8();
            for character in rest.chars().skip(1) {
                length = length.checked_add(character.len_utf8()).expect("source length");
                if character == '"' && !escaped {
                    break;
                }
                escaped = character == '\\' && !escaped;
            }
            length
        } else if character.is_whitespace() {
            character.len_utf8()
        } else {
            let length = if character.is_alphanumeric() || character == '_' {
                rest.chars()
                    .take_while(|character| character.is_alphanumeric() || *character == '_')
                    .map(char::len_utf8)
                    .sum()
            } else if rest.starts_with("::") {
                2
            } else {
                character.len_utf8()
            };
            result.push((at, at.checked_add(length).expect("source length")));
            length
        };
        at = at.checked_add(length).expect("source length");
    }
    result
}
