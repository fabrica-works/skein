//! Old golden values run through a kept patch (codec.md, 12.3).

use alloc::collections::{BTreeMap, BTreeSet};
use std::{fs, path::Path};

use skein_codec::patch::{Build, Capacity, Halt, Old, Patcher, PlanIx, Site, Step};
use skein_codec::schema::{self, DeclIx};
use skein_codec::value::{Node, Value};

use super::{
    Error,
    files::{Crate, golden_path},
    module::read_patch,
    text::{Block, PatchText},
};

/// The owner's written dispatch, called once at each written stop.
pub type Written<C> = fn(Site, &Old, &mut Build, Node, &mut C) -> Result<(), Halt>;

/// Checks a patch's golden results, or writes them for review when
/// `SKEIN_PATCH_WRITE=1`.
pub fn goldens<C: Default>(owner: &Path, patch: u32, written: Written<C>) -> Result<(), Error> {
    let write = std::env::var_os("SKEIN_PATCH_WRITE").is_some_and(|value| value == "1");
    run_goldens(&Crate::read(owner)?, patch, written, write)
}

pub(crate) fn run_goldens<C: Default>(
    krate: &Crate,
    number: u32,
    written: Written<C>,
    write: bool,
) -> Result<(), Error> {
    let mut failures = Vec::new();
    let Some(source) = krate.patches.get(&number) else {
        return Err(Error::Golden(vec![format!("patch {number}: patch text missing")]));
    };
    let patch = read_patch(source, number).map_err(|error| Error::Golden(vec![format!("patch {number}: {error}")]))?;
    for block in &patch.blocks {
        let label = format!("patch {number}, {}", block.family);
        if let Err(error) = family(krate, number, source, &patch, block, written, write, &mut failures) {
            failures.push(format!("{label}: {error}"));
        }
    }
    if failures.is_empty() { Ok(()) } else { Err(Error::Golden(failures)) }
}

fn kept<'a>(krate: &'a Crate, block: &Block, version: u16) -> Result<&'a str, String> {
    krate
        .kept
        .get(&block.family)
        .and_then(|versions| versions.get(&version))
        .map(|text| text.source.as_str())
        .ok_or_else(|| format!("kept version {version} missing"))
}

// Eight times the larger golden plus both schema texts and a kilobyte gives
// room for written answers. Nodes also count empty records, whose encodings
// need no bytes. An answer beyond this room reports a capacity failure.
fn capacity(old: &str, new: &str, source: &str, largest: usize, nodes: u32) -> Result<Capacity, String> {
    let text = u32::try_from(old.len().max(new.len()).max(source.len())).map_err(|error| error.to_string())?;
    if text > schema::MAX_TEXT {
        return Err("text exceeds the patcher's capacity".into());
    }
    let room = largest
        .checked_add(old.len())
        .and_then(|size| size.checked_add(new.len()))
        .and_then(|size| size.checked_mul(8))
        .and_then(|size| size.checked_add(1024))
        .and_then(|size| u32::try_from(size).ok())
        .ok_or("goldens exceed the patcher's capacity")?;
    let nodes = nodes
        .checked_mul(8)
        .and_then(|nodes| nodes.checked_add(1024))
        .ok_or("golden nodes exceed the patcher's capacity")?;
    Ok(Capacity { versions: 2, patches: 1, text, bytes: room, nodes: nodes.max(room) })
}

// Golden lists hold at most one item and golden options can be present.
// A named declaration refers backwards, so its node count is already known.
fn golden_nodes(schema: &crate::Schema) -> Result<u32, String> {
    let mut counts = BTreeMap::new();
    let mut largest = 1;
    for declaration in &schema.declarations {
        let (name, children) = match declaration {
            crate::Declaration::Record(record) => {
                let mut children = 0_u32;
                for field in &record.fields {
                    children =
                        children.checked_add(type_nodes(&field.ty, &counts)?).ok_or("golden node count exceeds u32")?;
                }
                (&record.name, children)
            }
            crate::Declaration::Enum(enumeration) => {
                let children = enumeration
                    .variants
                    .iter()
                    .filter_map(|variant| variant.record.as_ref())
                    .map(|name| *counts.get(name.as_str()).expect("earlier record"))
                    .max()
                    .unwrap_or(0);
                (&enumeration.name, children)
            }
        };
        let nodes = children.checked_add(1).ok_or("golden node count exceeds u32")?;
        counts.insert(name.as_str(), nodes);
        largest = largest.max(nodes);
    }
    Ok(largest)
}

fn type_nodes(ty: &crate::Type, counts: &BTreeMap<&str, u32>) -> Result<u32, String> {
    match ty {
        crate::Type::Named(name) => Ok(*counts.get(name.as_str()).expect("earlier declaration")),
        crate::Type::List(0, _)
        | crate::Type::U8
        | crate::Type::U16
        | crate::Type::U32
        | crate::Type::U64
        | crate::Type::Bool
        | crate::Type::Duration
        | crate::Type::Fixed(_)
        | crate::Type::Bytes(_)
        | crate::Type::Secret(_)
        | crate::Type::Text(_) => Ok(1),
        crate::Type::List(_, item) | crate::Type::Option(item) => {
            type_nodes(item, counts)?.checked_add(1).ok_or_else(|| "golden node count exceeds u32".into())
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "one family's run shares the patch text, dispatch and failure report")]
fn family<C: Default>(
    krate: &Crate,
    number: u32,
    source: &str,
    patch: &PatchText,
    block: &Block,
    written: Written<C>,
    write: bool,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    let old = kept(krate, block, block.from)?;
    let new = kept(krate, block, block.to)?;
    let old_schema = crate::parse(old).map_err(|error| error.to_string())?;
    let new_schema = crate::parse(new).map_err(|error| error.to_string())?;
    let nodes = golden_nodes(&old_schema)?.max(golden_nodes(&new_schema)?);
    let goldens = crate::emit_tests::goldens(&old_schema);
    let largest = goldens
        .iter()
        .map(|(_, golden)| golden.bytes.len())
        .chain(crate::emit_tests::goldens(&new_schema).iter().map(|(_, golden)| golden.bytes.len()))
        .max()
        .unwrap_or(0);
    let capacity = capacity(old, new, source, largest, nodes)?;
    let mut patcher = Patcher::new(capacity);
    patcher.keep(old.as_bytes()).map_err(|error| format!("keep {}: {error:?}", block.from))?;
    patcher.keep(new.as_bytes()).map_err(|error| format!("keep {}: {error:?}", block.to))?;
    let plan = patcher
        .plan(source.as_bytes())
        .map_err(|error| format!("line {}: {:?}", error.line, error.wrong))?
        .ok_or("no block planned")?;
    let mut decoded = Value::with_room(capacity.nodes);
    let mut results = BTreeMap::new();
    let mut expected = BTreeSet::new();
    for (root, golden) in goldens {
        let Some(new_root) = patcher.counterpart(plan, root) else {
            continue;
        };
        match run(&mut patcher, plan, root, golden.bytes.into_boxed_slice(), written) {
            Ok(bytes) => {
                expected.insert(golden.name.clone());
                match decoded.decode(
                    patcher.schema(block.to).expect("kept new text"),
                    new_root,
                    bytes.clone().into_boxed_slice(),
                ) {
                    Ok(_) => {
                        results.insert(golden.name, bytes);
                    }
                    Err(error) => {
                        failures
                            .push(format!("patch {number}, {}/{}: new value: {error:?}", block.family, golden.name));
                    }
                }
            }
            Err(Halt::Refused(refusal)) => {
                let name = format!("{}.refused", golden.name);
                expected.insert(name.clone());
                if let Some(reason) = patch.refusals().nth(usize::try_from(refusal.index()).expect("refusal index")) {
                    results.insert(name, format!("refuses {reason}\n").into_bytes());
                } else {
                    failures.push(format!(
                        "patch {number}, {}/{}: unknown refusal {}",
                        block.family,
                        golden.name,
                        refusal.index()
                    ));
                }
            }
            Err(error) => {
                // A failed run keeps its old result for review in write mode.
                expected.insert(golden.name.clone());
                expected.insert(format!("{}.refused", golden.name));
                failures.push(format!(
                    "patch {number}, {}/{}: {}",
                    block.family,
                    golden.name,
                    halt(error, patch, capacity)
                ));
            }
        }
    }
    let directory = krate.root.join(golden_path(number)).join(&block.family);
    compare(&directory, &results, &expected, write, failures);
    Ok(())
}

fn run<C: Default>(
    patcher: &mut Patcher,
    plan: PlanIx,
    root: DeclIx,
    bytes: Box<[u8]>,
    written: Written<C>,
) -> Result<Vec<u8>, Halt> {
    patcher.start(plan, root, bytes)?;
    loop {
        match patcher.next()? {
            Step::Written { site, at } => {
                let (old, new) = patcher.parts();
                written(site, old, new, at, &mut C::default())?;
            }
            Step::Held { at, .. } => {
                let (old, new) = patcher.parts();
                new.carry(old, at)?;
            }
            Step::Done => break,
        }
    }
    patcher.finish().map(<[u8]>::to_vec)
}

fn halt(error: Halt, patch: &PatchText, capacity: Capacity) -> String {
    match error {
        Halt::NoSite(site) => {
            let target = patch
                .sites()
                .nth(usize::try_from(site.index()).expect("site index"))
                .map_or("unknown", |(_, target, _)| target);
            format!("patch {} site {} (written {target})", site.patch(), site.index())
        }
        Halt::Room | Halt::Decode(skein_codec::value::Refusal::Room) => {
            format!("Room: golden exceeds capacity ({} bytes, {} nodes)", capacity.bytes, capacity.nodes)
        }
        Halt::Decode(_)
        | Halt::Refused(_)
        | Halt::Undeclared(_)
        | Halt::Misread(_)
        | Halt::Misbuilt(_)
        | Halt::Built(_)
        | Halt::Order => format!("{error:?}"),
    }
}

fn compare(
    directory: &Path,
    results: &BTreeMap<String, Vec<u8>>,
    expected: &BTreeSet<String>,
    write: bool,
    failures: &mut Vec<String>,
) {
    if write && let Err(error) = fs::create_dir_all(directory) {
        failures.push(format!("{}: {error}", directory.display()));
        return;
    }
    for (name, bytes) in results {
        let path = directory.join(name);
        if write {
            if let Err(error) = fs::write(&path, bytes) {
                failures.push(format!("{}: {error}", path.display()));
            }
        } else {
            match fs::read(&path) {
                Ok(held) if held == *bytes => {}
                Ok(_) => failures.push(format!("{}: differs", path.display())),
                Err(error) => failures.push(format!("{}: {error}", path.display())),
            }
        }
    }
    match fs::read_dir(directory) {
        Ok(entries) => {
            let mut paths = Vec::new();
            for entry in entries {
                match entry {
                    Ok(entry) => paths.push(entry.path()),
                    Err(error) => failures.push(format!("{}: {error}", directory.display())),
                }
            }
            paths.sort();
            for path in paths {
                if path.is_file()
                    && !path.file_name().and_then(|name| name.to_str()).is_some_and(|name| expected.contains(name))
                {
                    if write {
                        if let Err(error) = fs::remove_file(&path) {
                            failures.push(format!("{}: {error}", path.display()));
                        }
                    } else {
                        failures.push(format!("{}: stale", path.display()));
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => failures.push(format!("{}: {error}", directory.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::{Written, run_goldens};
    use crate::patch::{Error, files::Crate};
    use skein_codec::patch::{Build, Halt, Old, RefusalIx, Site};
    use skein_codec::schema::VariantIx;
    use skein_codec::value::Node;
    use std::{fs, path::PathBuf};

    const OLD: &str =
        "family small 1\nrecord Data { held: bytes 3 holds other Other }\nenum Shape { dot, line }\nrecord Drop {}\n";
    const NEW: &str =
        "family small 2\nrecord Data { held: bytes 3 holds other Other score: u8 }\nenum Shape { line }\n";
    const PATCH: &str = "patch 1\nowner sample 1\nfamily small 1 to 2\nadd Data.score u8 7\ndrop Shape.dot\nwritten Shape\nrefuses no_dot \"no dot\"\ndrop Drop\n";
    // Two variants, each with a smallest and a full golden, reach the site.
    const SHAPE_GOLDENS: usize = 4;
    // Two Data goldens and four Shape goldens; the dropped root has no file.
    const NAMES: [&str; 6] = [
        "enum_shape_dot_full.bin",
        "enum_shape_dot_smallest.bin",
        "enum_shape_line_full.bin",
        "enum_shape_line_smallest.bin",
        "record_data_full.bin",
        "record_data_smallest.bin",
    ];

    fn fixture(name: &str) -> Crate {
        let root = std::env::temp_dir().join(format!("skein-codegen-t2-golden-{name}-{}", std::process::id()));
        fs::create_dir(&root).expect("unique scratch directory");
        fs::create_dir_all(root.join("schema/kept")).expect("kept directory");
        fs::create_dir_all(root.join("patches")).expect("patch directory");
        for (path, text) in
            [("schema/kept/small-v1.schema", OLD), ("schema/kept/small-v2.schema", NEW), ("patches/0001.patch", PATCH)]
        {
            fs::write(root.join(path), text).expect("fixture text");
        }
        Crate::read(&root).expect("fixture crate")
    }

    fn answer(_site: Site, old: &Old, new: &mut Build, at: Node, _cx: &mut ()) -> Result<(), Halt> {
        if old.tag(at)? == VariantIx::new(0) { new.put_variant(VariantIx::new(0)) } else { new.carry(old, at) }
    }

    fn no_site(site: Site, _old: &Old, _new: &mut Build, _at: Node, _cx: &mut ()) -> Result<(), Halt> {
        Err(Halt::NoSite(site))
    }

    fn refuse(_site: Site, old: &Old, new: &mut Build, at: Node, _cx: &mut ()) -> Result<(), Halt> {
        if old.tag(at)? == VariantIx::new(0) { Err(new.refuse(RefusalIx::new(0))) } else { new.carry(old, at) }
    }

    fn directory(krate: &Crate) -> PathBuf {
        krate.root.join("patches/golden/0001/small")
    }

    fn written(name: &str, dispatch: Written<()>) -> Crate {
        let krate = fixture(name);
        run_goldens(&krate, 1, dispatch, true).expect("write goldens");
        krate
    }

    #[test]
    fn empty_record_trees_get_nodes_beyond_their_bytes() {
        use core::fmt::Write;
        // Fifteen binary levels above Empty have 65,535 nodes and no bytes.
        const LEVELS: u32 = 15;
        const TREE_NODES: u32 = 65_535;
        let mut text = String::from("family empty 1\nrecord D0 {}\n");
        for level in 1..=LEVELS {
            let previous = level.checked_sub(1).expect("positive level");
            writeln!(text, "record D{level} {{ a: D{previous} b: D{previous} }}").expect("schema text");
        }
        let schema = crate::parse(&text).expect("empty tree schema");
        let nodes = super::golden_nodes(&schema).expect("node bound");
        assert_eq!(nodes, TREE_NODES);
        let capacity = super::capacity(&text, &text, "patch 1\nowner sample 1\n", 0, nodes).expect("capacity");
        assert!(capacity.nodes > TREE_NODES, "empty trees need room despite their zero bytes");
    }

    #[test]
    fn writes_goldens_carries_held_fields_and_checks_new_values() {
        let krate = written("write", answer);
        let mut names = fs::read_dir(directory(&krate))
            .expect("goldens")
            .map(|entry| entry.expect("file").file_name().to_str().expect("UTF-8").to_owned())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, NAMES);
        assert_eq!(fs::read(directory(&krate).join("record_data_smallest.bin")).expect("smallest"), b"\0\0\0\0\x07");
        assert_eq!(
            fs::read(directory(&krate).join("record_data_full.bin")).expect("full"),
            b"\0\0\0\x03\0\x7f\xff\x07"
        );
        assert_eq!(run_goldens(&krate, 1, answer, false), Ok(()));
        fs::remove_dir_all(krate.root).expect("remove fixture");
    }

    #[test]
    fn missing_changed_and_stale_goldens_are_all_named() {
        // One changed, one missing and one stale file.
        const DRIFTED_FILES: usize = 3;
        let krate = written("drift", answer);
        let directory = directory(&krate);
        fs::write(directory.join("record_data_full.bin"), b"changed").expect("changed");
        fs::remove_file(directory.join("record_data_smallest.bin")).expect("missing");
        fs::write(directory.join("extra.bin"), b"stale").expect("stale");
        let Error::Golden(failures) = run_goldens(&krate, 1, answer, false).expect_err("all drift fails") else {
            unreachable!("golden failures")
        };
        assert_eq!(failures.len(), DRIFTED_FILES);
        for name in ["record_data_full.bin", "record_data_smallest.bin", "extra.bin"] {
            assert!(failures.iter().any(|failure| failure.contains(name)), "{name}: {failures:?}");
        }
        fs::remove_dir_all(krate.root).expect("remove fixture");
    }

    #[test]
    fn write_removes_results_without_a_surviving_golden() {
        let krate = written("stale", answer);
        for name in ["extra.bin", "record_drop_full.bin", "enum_shape_dot_full.bin.refused"] {
            fs::write(directory(&krate).join(name), b"stale").expect("stale");
        }
        run_goldens(&krate, 1, answer, true).expect("rewrite removes stale files");
        assert_eq!(fs::read_dir(directory(&krate)).expect("goldens").count(), NAMES.len());
        fs::remove_dir_all(krate.root).expect("remove fixture");
    }

    #[test]
    fn missing_written_functions_name_patch_site_and_target() {
        let krate = fixture("no-site");
        let Error::Golden(failures) = run_goldens(&krate, 1, no_site, false).expect_err("unanswered sites") else {
            unreachable!("golden failures")
        };
        let sites =
            failures.iter().filter(|failure| failure.contains("patch 1 site 0 (written Shape)")).collect::<Vec<_>>();
        assert_eq!(sites.len(), SHAPE_GOLDENS);
        fs::remove_dir_all(krate.root).expect("remove fixture");
    }

    #[test]
    fn declared_refusals_are_written_and_checked_as_refused_files() {
        let krate = written("refused", refuse);
        for name in ["enum_shape_dot_full.bin.refused", "enum_shape_dot_smallest.bin.refused"] {
            assert_eq!(fs::read_to_string(directory(&krate).join(name)).expect("refused golden"), "refuses no_dot\n");
        }
        assert_eq!(run_goldens(&krate, 1, refuse, false), Ok(()));
        run_goldens(&krate, 1, answer, true).expect("change refusal to success");
        assert_eq!(fs::read_dir(directory(&krate)).expect("goldens").count(), NAMES.len());
        fs::remove_dir_all(krate.root).expect("remove fixture");
    }

    #[test]
    fn planning_refusals_name_patch_family_line_and_reason() {
        let mut krate = fixture("plan");
        krate.patches.insert(1, "patch 1\nowner sample 1\nfamily small 1 to 2\nadd Data.score u8 ?\n".into());
        let Error::Golden(failures) = run_goldens(&krate, 1, answer, false).expect_err("asked value") else {
            unreachable!("golden failures")
        };
        assert_eq!(failures, ["patch 1, small: line 4: Asked"]);
        fs::remove_dir_all(krate.root).expect("remove fixture");
    }
}
