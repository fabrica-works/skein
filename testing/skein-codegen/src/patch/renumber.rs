//! Move the author's files away from a number taken by the base.

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

fn shift_versions(source: &str, before: &PatchText, after: &PatchText) -> String {
    let mut result = source.to_owned();
    for old in &before.blocks {
        let Some(new) = after.blocks.iter().find(|block| block.family == old.family) else { continue };
        if old.from == new.from {
            continue;
        }
        let family = old.family.replace('-', "_");
        // Tokenize both old versions in one pass so adjacent shifts cannot cascade.
        let prefix = format!("{family}::v");
        let mut shifted = String::new();
        let mut rest = result.as_str();
        while let Some(at) = rest.find(&prefix) {
            let (head, tail) = rest.split_at(at);
            let boundary =
                head.chars().next_back().is_none_or(|character| !character.is_alphanumeric() && character != '_');
            shifted.push_str(head);
            shifted.push_str(&prefix);
            let tail = tail.strip_prefix(&prefix).expect("prefix found");
            let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
            let (version, tail) = tail.split_at(digits);
            let replacement = if boundary && tail.starts_with("::") && version.parse::<u16>() == Ok(old.from) {
                new.from.to_string()
            } else if boundary && tail.starts_with("::") && version.parse::<u16>() == Ok(old.to) {
                new.to.to_string()
            } else {
                version.to_owned()
            };
            shifted.push_str(&replacement);
            rest = tail;
        }
        shifted.push_str(rest);
        result = shifted;
    }
    result
}
