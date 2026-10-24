//! Each landing supplies the next run's base (codec.md, 12.9).

extern crate alloc;

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;
    use skein_codec::patch::{Build, Halt, Old, Site};
    use skein_codec::value::Node;
    use skein_codec_patches::patches::{self, Context};
    use skein_codegen::diff::Rename;
    use skein_codegen::patch::{self, Error, Finding, Landed, Mode, Options, Outcome, Report};
    use std::fs;
    use std::path::{Path, PathBuf};

    const BASELINE: &str = "01-baseline";
    const FIELD: &str = "02-field";
    const RENAMES: &str = "03-renames";
    const WRITTEN: &str = "04-written";
    const RENUMBERED: &str = "05-renumbered";
    // Two records and three enum variants, each smallest and full.
    const SHELF_GOLDENS: [&str; 10] = [
        "enum_format_audio_full.bin",
        "enum_format_audio_smallest.bin",
        "enum_format_ebook_full.bin",
        "enum_format_ebook_smallest.bin",
        "enum_format_paper_full.bin",
        "enum_format_paper_smallest.bin",
        "record_book_full.bin",
        "record_book_smallest.bin",
        "record_shelf_full.bin",
        "record_shelf_smallest.bin",
    ];
    // One Label record, smallest and full.
    const LABEL_GOLDENS: [&str; 2] = ["record_label_full.bin", "record_label_smallest.bin"];

    #[derive(Debug)]
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("skein-codec-patches-{name}-{}", std::process::id()));
            fs::create_dir(&root).expect("unique story scratch directory");
            Self(root)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove story scratch directory");
        }
    }
    fn owner() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }
    fn input(step: &str, name: &str) -> PathBuf {
        owner().join("story").join(step).join(name)
    }
    fn put(root: &Path, path: &Path, bytes: &[u8]) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("file parent")).expect("story directories");
        fs::write(path, bytes).expect("story file");
    }
    fn author(root: &Path, path: &str, step: &str, name: &str) {
        put(root, Path::new(path), &fs::read(input(step, name)).expect("author's input"));
    }
    fn text(root: &Path, path: &str, step: &str, name: &str) {
        assert_eq!(
            fs::read_to_string(root.join(path)).expect("command's text"),
            fs::read_to_string(input(step, name)).expect("expected text"),
            "{step}: {path}"
        );
    }
    fn files(root: &Path, directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut result = BTreeMap::new();
        let full = root.join(directory);
        if full.is_dir() {
            for entry in fs::read_dir(full).expect("story directory") {
                let entry = entry.expect("story entry");
                let path = directory.join(entry.file_name());
                if entry.path().is_dir() {
                    result.extend(files(root, &path));
                } else {
                    result.insert(path, fs::read(entry.path()).expect("story file"));
                }
            }
        }
        result
    }
    fn restore(root: &Path, landed: &Landed) {
        for path in landed.paths() {
            put(root, path, landed.get(path).expect("landed bytes"));
        }
    }
    fn clean(root: &Path, landed: &Landed) {
        assert_eq!(patch::check(root, landed), Ok(vec![]), "step has no findings");
        assert_eq!(patch::current(root), Ok(()), "step has the generated module");
    }
    fn golden_names<const N: usize>(root: &Path, directory: &str, names: [&str; N]) {
        assert_eq!(
            files(root, Path::new(directory)).keys().cloned().collect::<Vec<_>>(),
            names.map(|name| Path::new(directory).join(name)),
            "{directory}: every golden, no stale files"
        );
    }
    fn current(family: &str) -> Outcome {
        Outcome::Current { family: family.into() }
    }
    fn shelf_patch(from: u16, to: u16) -> Outcome {
        Outcome::Patched { family: "shelf".into(), from, to }
    }
    fn rename(family: &str, decl: &str, old: &str, new: &str) -> Options {
        Options {
            renames: vec![(family.into(), Rename::Part { decl: decl.into(), old: old.into(), new: new.into() })],
            ..Options::default()
        }
    }
    fn skeleton(site: Site, _old: &Old, _new: &mut Build, _at: Node, _cx: &mut Context) -> Result<(), Halt> {
        Err(Halt::NoSite(site))
    }
    // Step four's patch is still 2; the compiled author's module is its final 3.
    fn written(site: Site, old: &Old, new: &mut Build, at: Node, cx: &mut Context) -> Result<(), Halt> {
        patches::written(Site::new(3, site.index()), old, new, at, cx)
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "codec.md 12.9 asks for one readable history, each run checked beside its inputs"
    )]
    fn owner_history_replays_from_baseline_through_rebased_written_patch() {
        let scratch = Scratch::new("branch");
        let root = &scratch.0;
        let options = Options::default();

        // 1. Before release the command requires an explicit baseline.
        author(root, "schema/shelf-v1.schema", BASELINE, "shelf-v1.schema");
        author(root, "schema/label-v1.schema", BASELINE, "label-v1.schema");
        let before = files(root, Path::new(""));
        assert_eq!(patch::run(root, &Landed::none(), &options), Err(Error::Baseline));
        assert_eq!(files(root, Path::new("")), before, "refused run writes nothing");
        assert_eq!(
            patch::run(root, &Landed::none(), &Options { baseline: true, ..Options::default() }),
            Ok(Report {
                outcomes: vec![
                    Outcome::Kept { family: "label".into(), version: 1 },
                    Outcome::Kept { family: "shelf".into(), version: 1 }
                ],
                ..Report::default()
            })
        );
        text(root, "schema/kept/shelf-v1.schema", BASELINE, "shelf-v1.schema");
        text(root, "schema/kept/label-v1.schema", BASELINE, "label-v1.schema");
        text(root, "patches/mod.rs", BASELINE, "mod.rs");
        text(root, "patches/context.rs", BASELINE, "context.rs");
        clean(root, &Landed::none());
        let baseline = Landed::read(root).expect("baseline lands");

        // 2. A new field first asks its author, then plans with zero pages.
        author(root, "schema/shelf-v2.schema", FIELD, "shelf-v2.schema");
        let unplanned =
            Finding::Unplanned { patch: 1, family: "shelf".into(), error: "line 5 at 62..63: Asked".into() };
        assert_eq!(
            patch::run(root, &baseline, &Options { owner: Some("skein".into()), ..Options::default() }),
            Ok(Report {
                outcomes: vec![current("label"), shelf_patch(1, 2)],
                patch: Some(1),
                asked: vec!["patch 0001, shelf: add Book.pages u16 ?".into()],
                findings: vec![unplanned.clone()],
                ..Report::default()
            })
        );
        text(root, "patches/0001.patch", FIELD, "asked.patch");
        text(root, "schema/kept/shelf-v2.schema", FIELD, "shelf-v2.schema");
        text(root, "patches/mod.rs", FIELD, "mod.rs");
        assert_eq!(patch::check(root, &baseline), Ok(vec![unplanned]));
        assert!(!root.join("patches/golden/0001").exists(), "an unplanned patch writes no goldens");
        author(root, "patches/0001.patch", FIELD, "0001.patch");
        assert_eq!(
            patch::run(root, &baseline, &options),
            Ok(Report { outcomes: vec![current("label"), shelf_patch(1, 2)], patch: Some(1), ..Report::default() })
        );
        text(root, "patches/0001.patch", FIELD, "0001.patch");
        text(root, "patches/mod.rs", FIELD, "mod.rs");
        golden_names(root, "patches/golden/0001/shelf", SHELF_GOLDENS);
        assert_eq!(patch::golden(root, 1, patches::written, Mode::Check), Ok(()));
        clean(root, &baseline);
        let field = Landed::read(root).expect("added pages lands");

        // 3. Label can refresh; shelf's frozen v2 keeps its rename waiting.
        author(root, "schema/label-v1.schema", RENAMES, "label-v1.schema");
        assert_eq!(
            patch::run(root, &field, &rename("label", "Label", "text", "caption")),
            Ok(Report {
                outcomes: vec![Outcome::Refreshed { family: "label".into(), version: 1 }, current("shelf")],
                ..Report::default()
            })
        );
        text(root, "schema/kept/label-v1.schema", RENAMES, "label-v1.schema");
        text(root, "patches/0001.patch", FIELD, "0001.patch");
        text(root, "patches/mod.rs", RENAMES, "mod.rs");
        clean(root, &field);
        author(root, "schema/shelf-v2.schema", RENAMES, "shelf-v2.schema");
        let waiting = Outcome::Waiting {
            family: "shelf".into(),
            renames: vec![Rename::Part { decl: "Book".into(), old: "title".into(), new: "name".into() }],
        };
        assert_eq!(
            patch::run(root, &field, &rename("shelf", "Book", "title", "name")),
            Ok(Report { outcomes: vec![current("label"), waiting.clone()], ..Report::default() })
        );
        text(root, "schema/kept/shelf.renames", RENAMES, "shelf.renames");
        text(root, "schema/kept/shelf-v2.schema", FIELD, "shelf-v2.schema");
        text(root, "patches/mod.rs", RENAMES, "mod.rs");
        clean(root, &field);
        let renames = Landed::read(root).expect("renames land");

        // 4. Audio is dropped. The skeleton reports its site; the author then
        // replaces it and reviews the results that will live under 0003.
        author(root, "schema/shelf-v3.schema", WRITTEN, "shelf-v3.schema");
        assert_eq!(
            patch::run(root, &renames, &options),
            Ok(Report {
                outcomes: vec![current("label"), shelf_patch(2, 3)],
                patch: Some(2),
                asked: vec!["patch 0002, shelf: written Format".into()],
                ..Report::default()
            })
        );
        text(root, "patches/0002.patch", WRITTEN, "0002.patch");
        text(root, "patches/p0002.rs", WRITTEN, "skeleton.rs");
        text(root, "patches/mod.rs", WRITTEN, "mod.rs");
        text(root, "schema/kept/shelf-v3.schema", WRITTEN, "shelf-v3.schema");
        assert!(!root.join("schema/kept/shelf.renames").exists(), "patch consumes waiting rename");
        clean(root, &renames);
        let Err(Error::Golden(failures)) = patch::golden(root, 2, skeleton, Mode::Check) else {
            unreachable!("skeleton must fail its goldens")
        };
        // Successful Book conversions also report their two not-yet-reviewed files.
        // Keep the path and omit the platform's filesystem error wording.
        let failures = failures
            .iter()
            .map(|failure| {
                failure.strip_prefix(&format!("{}/", root.display())).map_or_else(
                    || failure.clone(),
                    |message| message.split_once(": ").expect("file failure has its reason").0.to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let mut expected = [
            "enum_format_paper_smallest.bin",
            "enum_format_paper_full.bin",
            "enum_format_audio_smallest.bin",
            "enum_format_audio_full.bin",
            "enum_format_ebook_smallest.bin",
            "enum_format_ebook_full.bin",
            "record_shelf_smallest.bin",
            "record_shelf_full.bin",
        ]
        .map(|name| format!("patch 2, shelf/{name}: patch 2 site 0 (written Format)"))
        .to_vec();
        expected.extend([
            "patches/golden/0002/shelf/record_book_full.bin".into(),
            "patches/golden/0002/shelf/record_book_smallest.bin".into(),
        ]);
        assert_eq!(failures, expected);
        author(root, "patches/p0002.rs", WRITTEN, "p0002.rs");
        for (path, bytes) in files(owner(), Path::new("patches/golden/0003/shelf")) {
            put(root, &Path::new("patches/golden/0002/shelf").join(path.file_name().expect("golden name")), &bytes);
        }
        golden_names(root, "patches/golden/0002/shelf", SHELF_GOLDENS);
        assert_eq!(patch::golden(root, 2, written, Mode::Check), Ok(()));

        // 5. Main lands label's color while the branch's written change waits.
        // This scratch tree is the branch's side of patches/ and schema/kept/;
        // the current schemas merge main's label v2 before the command runs.
        let main_scratch = Scratch::new("main");
        let main_root = &main_scratch.0;
        restore(main_root, &renames);
        author(main_root, "schema/shelf-v2.schema", RENAMES, "shelf-v2.schema");
        author(main_root, "schema/label-v2.schema", RENUMBERED, "label-v2.schema");
        let label_asked = patch::run(main_root, &renames, &options).expect("main asks color");
        assert_eq!(label_asked.asked, ["patch 0002, label: add Label.color u8 ?"]);
        author(main_root, "patches/0002.patch", RENUMBERED, "0002.patch");
        assert_eq!(
            patch::run(main_root, &renames, &options),
            Ok(Report {
                outcomes: vec![Outcome::Patched { family: "label".into(), from: 1, to: 2 }, waiting],
                patch: Some(2),
                ..Report::default()
            })
        );
        text(main_root, "patches/0002.patch", RENUMBERED, "0002.patch");
        text(main_root, "schema/kept/label-v2.schema", RENUMBERED, "label-v2.schema");
        text(main_root, "patches/mod.rs", RENUMBERED, "mod.rs");
        golden_names(main_root, "patches/golden/0002/label", LABEL_GOLDENS);
        clean(main_root, &renames);
        let main = Landed::read(main_root).expect("main's color lands first");
        author(root, "schema/label-v2.schema", RENUMBERED, "label-v2.schema");
        assert_eq!(
            patch::run(root, &main, &options),
            Ok(Report {
                outcomes: vec![current("label"), shelf_patch(2, 3)],
                patch: Some(3),
                moved: Some(2),
                asked: vec!["patch 0003, shelf: written Format".into()],
                restored: vec![
                    "patches/0002.patch".into(),
                    "patches/golden/0002/label/record_label_full.bin".into(),
                    "patches/golden/0002/label/record_label_smallest.bin".into(),
                    "schema/kept/label-v2.schema".into()
                ],
                ..Report::default()
            })
        );
        text(root, "patches/0002.patch", RENUMBERED, "0002.patch");
        assert!(!root.join("patches/p0002.rs").exists(), "written function moved");
        assert!(!root.join("patches/golden/0002/shelf").exists(), "reviewed shelf goldens moved");
        golden_names(root, "patches/golden/0003/shelf", SHELF_GOLDENS);
        golden_names(root, "patches/golden/0002/label", LABEL_GOLDENS);
        clean(root, &main);
        for directory in ["schema", "patches"] {
            let replayed = files(root, Path::new(directory));
            let committed = files(owner(), Path::new(directory));
            assert_eq!(
                replayed.keys().collect::<Vec<_>>(),
                committed.keys().collect::<Vec<_>>(),
                "final {directory} paths"
            );
            for (path, bytes) in &committed {
                let result = replayed.get(path).expect("same paths");
                if let (Ok(result), Ok(bytes)) = (core::str::from_utf8(result), core::str::from_utf8(bytes)) {
                    assert_eq!(result, bytes, "final {}", path.display());
                } else {
                    assert_eq!(result, bytes, "final {}", path.display());
                }
            }
        }
    }
}
