//! Author-owned written functions and context (codec.md, 12.3).

use core::fmt::Write;

use super::{
    Error,
    module::function_names,
    text::{Line, PatchText},
};

pub(crate) const CONTEXT: &str = "//! The owner's per-record inputs to written patch functions (codec.md, 11.5): the command wrote it once; it is the owner's.\n\n#[derive(Debug, Default)]\npub struct Context {}\n";

pub(crate) fn skeleton_text(number: u32, patch: &PatchText, existing: Option<&str>) -> Result<String, Error> {
    let names = function_names(patch)?;
    let mut out = existing.map_or_else(|| format!("//! Patch {number:04}: the command wrote the skeleton; its functions are the author's (codec.md, 11.5).\n"), str::to_owned);
    if existing.is_none() && !names.is_empty() {
        out.push_str(
            "\nuse super::Context;\nuse skein_codec::patch::{Build, Halt, Old, Site};\nuse skein_codec::value::Node;\n",
        );
    }
    for (index, ((block, target, refusals), name)) in patch.sites().zip(names).enumerate() {
        if existing.is_some_and(|text| text.contains(&format!("pub fn {name}("))) {
            continue;
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }
        writeln!(out, "\n/// Site {index}, `written {target}`, {} {} to {}.\n///", block.family, block.from, block.to)
            .expect("String write");
        let declaration = target.split('.').next().expect("site target");
        for line in &block.lines {
            if let Line::Op(line) = line {
                let operation_target = line.split_whitespace().nth(1).unwrap_or_default();
                if operation_target == declaration
                    || operation_target.strip_prefix(declaration).is_some_and(|rest| rest.starts_with('.'))
                {
                    writeln!(out, "/// - `{line}`").expect("String write");
                }
            }
        }
        for refusal in refusals {
            writeln!(out, "/// - `refuses {} {}`", refusal.name, refusal.text).expect("String write");
        }
        write!(out, "pub fn {name}(_old: &Old, _new: &mut Build, _at: Node, _cx: &mut Context) -> Result<(), Halt> {{\n    Err(Halt::NoSite(Site::new({number}, {index})))\n}}\n").expect("String write");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{CONTEXT, skeleton_text};
    use crate::patch::text::PatchText;
    const PATCH: &str = "patch 1\nowner sample 1\n\nfamily a-b 1 to 2\ndrop Shape.dot\nwritten Shape\nrefuses no_dot \"no dot\"\nretype Every.score u8 u16\nwritten Every.score\nrefuses no_score \"no score\"\n";
    const SKELETON: &str = r#"//! Patch 0001: the command wrote the skeleton; its functions are the author's (codec.md, 11.5).

use super::Context;
use skein_codec::patch::{Build, Halt, Old, Site};
use skein_codec::value::Node;

/// Site 0, `written Shape`, a-b 1 to 2.
///
/// - `drop Shape.dot`
/// - `refuses no_dot "no dot"`
pub fn shape(_old: &Old, _new: &mut Build, _at: Node, _cx: &mut Context) -> Result<(), Halt> {
    Err(Halt::NoSite(Site::new(1, 0)))
}

/// Site 1, `written Every.score`, a-b 1 to 2.
///
/// - `retype Every.score u8 u16`
/// - `refuses no_score "no score"`
pub fn every_score(_old: &Old, _new: &mut Build, _at: Node, _cx: &mut Context) -> Result<(), Halt> {
    Err(Halt::NoSite(Site::new(1, 1)))
}
"#;

    #[test]
    fn skeleton_pins_sites_operations_and_declared_refusals() {
        assert_eq!(skeleton_text(1, &PatchText::read(PATCH).expect("patch"), None).expect("skeleton"), SKELETON);
        assert_eq!(
            CONTEXT,
            "//! The owner's per-record inputs to written patch functions (codec.md, 11.5): the command wrote it once; it is the owner's.\n\n#[derive(Debug, Default)]\npub struct Context {}\n"
        );
    }

    #[test]
    fn skeleton_preserves_authored_functions_and_appends_missing_ones() {
        let existing = "// the author's text\npub fn shape(\n    // signature and body belong to the author\n) {}";
        let expected = concat!(
            "// the author's text\npub fn shape(\n    // signature and body belong to the author\n) {}\n\n",
            "/// Site 1, `written Every.score`, a-b 1 to 2.\n///\n",
            "/// - `retype Every.score u8 u16`\n/// - `refuses no_score \"no score\"`\n",
            "pub fn every_score(_old: &Old, _new: &mut Build, _at: Node, _cx: &mut Context) -> Result<(), Halt> {\n",
            "    Err(Halt::NoSite(Site::new(1, 1)))\n}\n"
        );
        let patch = PatchText::read(PATCH).expect("patch");
        assert_eq!(skeleton_text(1, &patch, Some(existing)).expect("appended"), expected);
        assert_eq!(skeleton_text(1, &patch, Some(SKELETON)).expect("complete"), SKELETON);
    }
}
