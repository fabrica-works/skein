//! Whole-file examples of the run, with the base supplied in memory.
extern crate alloc;

#[cfg(test)]
mod tests {

    use alloc::collections::BTreeMap;
    use skein_codec::patch::{Build, Halt, Old, Site};
    use skein_codec::value::Node;
    use skein_codegen::diff::{Rename, Version};
    use skein_codegen::patch::{Error, Landed, Mode, Options, Outcome, current, golden, run};
    use std::fs;
    use std::path::{Path, PathBuf};

    const V1: &str = "family shelf 1\nrecord Book { id: u8 }\nenum State { ready }\n";
    const V2: &str = "family shelf 2\nrecord Book { id: u8, score: u32 }\nenum State { ready }\n";
    const OPTIONAL: &str = "family shelf 2\nrecord Book { id: u8, note: option text 8 }\nenum State { ready }\n";
    const RENAMED: &str = "family shelf 1\nrecord Book { key: u8 }\nenum State { ready }\n";
    const LABEL: &str = "family label 4\nrecord Label { id: u8 }\n";
    const PATCH: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nadd Book.score u32 ?\n";
    const FILLED: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nadd Book.score u32 7\n";
    const FROZEN: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nadd Book.note option text 8 none\n";

    fn crate_files(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("skein-patch-run-{}-{name}", std::process::id()));
        fs::create_dir(&root).expect("unique scratch crate");
        write_files(&root, files);
        root
    }

    fn write_files(root: &Path, files: &[(&str, &str)]) {
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().expect("file parent")).expect("directories");
            fs::write(path, text).expect("fixture file");
        }
    }

    fn assert_files(root: &Path, files: &[(&str, &str)]) {
        for (path, text) in files {
            assert_eq!(fs::read_to_string(root.join(path)).expect(path), *text, "{path}");
        }
    }

    fn landed(files: &[(&str, &str)]) -> Landed {
        Landed::new(files.iter().map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec())).collect())
    }

    fn options() -> Options {
        Options { owner: Some("shop".into()), ..Options::default() }
    }

    fn end(root: &Path) {
        fs::remove_dir_all(root).expect("remove scratch crate");
    }

    fn frozen_crate(name: &str, current: &str) -> (PathBuf, Landed) {
        let files = [
            ("schema/kept/shelf-v1.schema", V1),
            ("schema/kept/shelf-v2.schema", OPTIONAL),
            ("patches/0001.patch", FROZEN),
        ];
        let root = crate_files(name, &files);
        write_files(&root, &[("schema/shelf-v2.schema", current)]);
        (root, landed(&files))
    }

    #[test]
    fn before_baseline_requires_flag_and_writes_nothing() {
        let files = [("schema/shelf-v1.schema", V1)];
        let root = crate_files("before", &files);
        assert_eq!(run(&root, &Landed::none(), &options()), Err(Error::Baseline));
        assert_files(&root, &files);
        assert!(!root.join("schema/kept").exists(), "no baseline written");
        end(&root);
    }

    #[test]
    fn baseline_keeps_two_families_in_name_order() {
        let root = crate_files("baseline", &[("schema/shelf-v1.schema", V1), ("schema/label-v4.schema", LABEL)]);
        let report = run(&root, &Landed::none(), &Options { baseline: true, ..options() }).expect("baseline");
        assert_eq!(
            report.outcomes,
            [
                Outcome::Kept { family: "label".into(), version: 4 },
                Outcome::Kept { family: "shelf".into(), version: 1 }
            ]
        );
        assert_files(&root, &[("schema/kept/shelf-v1.schema", V1), ("schema/kept/label-v4.schema", LABEL)]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn family_added_past_baseline_needs_no_flag() {
        let root = crate_files(
            "new-family",
            &[("schema/shelf-v1.schema", V1), ("schema/kept/shelf-v1.schema", V1), ("schema/label-v4.schema", LABEL)],
        );
        let report = run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("new family");
        assert_eq!(
            report.outcomes,
            [Outcome::Kept { family: "label".into(), version: 4 }, Outcome::Current { family: "shelf".into() }]
        );
        assert_files(&root, &[("schema/kept/label-v4.schema", LABEL)]);
        end(&root);
    }

    #[test]
    fn branch_only_kept_texts_are_kept_afresh() {
        let root = crate_files("afresh", &[("schema/shelf-v2.schema", V2), ("schema/kept/shelf-v1.schema", V1)]);
        let report = run(&root, &Landed::none(), &options()).expect("new baseline version");
        assert_eq!(report.outcomes, [Outcome::Kept { family: "shelf".into(), version: 2 }]);
        assert_files(&root, &[("schema/kept/shelf-v2.schema", V2)]);
        assert!(!root.join("schema/kept/shelf-v1.schema").exists(), "branch's other text removed");
        end(&root);
    }

    #[test]
    fn unfrozen_kept_text_refreshes_bytes_and_confirmed_names() {
        let root = crate_files("refresh", &[("schema/shelf-v1.schema", RENAMED), ("schema/kept/shelf-v1.schema", V1)]);
        let report = run(
            &root,
            &landed(&[("schema/kept/shelf-v1.schema", V1)]),
            &Options {
                renames: vec![(
                    "shelf".into(),
                    Rename::Part { decl: "Book".into(), old: "id".into(), new: "key".into() },
                )],
                ..options()
            },
        )
        .expect("refresh");
        assert_eq!(report.outcomes, [Outcome::Refreshed { family: "shelf".into(), version: 1 }]);
        assert_files(&root, &[("schema/kept/shelf-v1.schema", RENAMED)]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn unchanged_kept_text_is_current() {
        let root = crate_files("current", &[("schema/shelf-v1.schema", V1), ("schema/kept/shelf-v1.schema", V1)]);
        let report = run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("current");
        assert_eq!(report.outcomes, [Outcome::Current { family: "shelf".into() }]);
        assert_files(&root, &[("schema/kept/shelf-v1.schema", V1)]);
        end(&root);
    }

    #[test]
    fn frozen_renames_wait_then_next_patch_takes_them() {
        const WAITING: &str = "family shelf 2\nrecord Book { key: u8, note: option text 8 }\nenum State { ready }\n";
        const NEXT: &str =
            "family shelf 3\nrecord Book { key: u8, note: option text 8, score: option u8 }\nenum State { ready }\n";
        const NEXT_PATCH: &str =
            "patch 1\nowner shop 2\n\nfamily shelf 2 to 3\nrename Book.id key\nadd Book.score option u8 none\n";
        let (root, base) = frozen_crate("waiting", WAITING);
        let report = run(
            &root,
            &base,
            &Options {
                renames: vec![(
                    "shelf".into(),
                    Rename::Part { decl: "Book".into(), old: "id".into(), new: "key".into() },
                )],
                ..options()
            },
        )
        .expect("waiting");
        assert_eq!(
            report.outcomes,
            [Outcome::Waiting {
                family: "shelf".into(),
                renames: vec![Rename::Part { decl: "Book".into(), old: "id".into(), new: "key".into() }]
            }]
        );
        assert_files(
            &root,
            &[("schema/kept/shelf.renames", "rename Book.id key\n"), ("schema/kept/shelf-v2.schema", OPTIONAL)],
        );
        write_files(&root, &[("schema/shelf-v3.schema", NEXT)]);
        let report = run(&root, &base, &options()).expect("next patch takes renames");
        assert_files(&root, &[("patches/0002.patch", NEXT_PATCH), ("schema/kept/shelf-v3.schema", NEXT)]);
        assert!(!root.join("schema/kept/shelf.renames").exists(), "waiting file consumed");
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn derived_patch_asks_for_added_fields_and_keeps_current() {
        let root = crate_files("derived", &[("schema/shelf-v2.schema", V2), ("schema/kept/shelf-v1.schema", V1)]);
        let report = run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("derived patch");
        assert_files(&root, &[("patches/0001.patch", PATCH), ("schema/kept/shelf-v2.schema", V2)]);
        assert_eq!(report.outcomes, [Outcome::Patched { family: "shelf".into(), from: 1, to: 2 }]);
        assert_eq!(report.asked, ["patch 0001, shelf: add Book.score u32 ?"]);
        assert_eq!(report.findings.len(), 1, "asked value does not plan");
        end(&root);
    }

    #[test]
    fn written_change_places_its_site_after_declaration() {
        const WRITTEN: &str = "family shelf 2\nrecord Book { id: bool }\nenum State { ready }\n";
        const TEXT: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nretype Book.id u8 bool\nwritten Book\n";
        let root = crate_files("written", &[("schema/shelf-v2.schema", WRITTEN), ("schema/kept/shelf-v1.schema", V1)]);
        let report = run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("written patch");
        assert_files(&root, &[("patches/0001.patch", TEXT)]);
        assert_eq!(report.asked, ["patch 0001, shelf: written Book"]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn an_added_field_that_takes_no_value_is_written_at_its_own_site() {
        const KEPT: &str = "family shelf 1\nrecord Label { id: u8 }\nenum State { ready }\nenum Held { label: Label }\nrecord Book { id: u8 }\n";
        const GROWN: &str = "family shelf 2\nrecord Label { id: u8 }\nenum State { ready }\nenum Held { label: Label }\nrecord Book { id: u8, label: Label, key: secret 16, sealed: bytes 8 holds other Thing, held: Held, state: State, count: u32 }\n";
        const TEXT: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nadd Book.label Label written\nadd Book.key secret 16 written\nadd Book.sealed bytes 8 holds other Thing written\nadd Book.held Held written\nadd Book.state State ?\nadd Book.count u32 ?\nwritten Book.label\nwritten Book.key\nwritten Book.sealed\nwritten Book.held\n";
        let filled = TEXT.replace("State ?", "State ready").replace("u32 ?", "u32 7");
        let root =
            crate_files("valueless", &[("schema/shelf-v2.schema", GROWN), ("schema/kept/shelf-v1.schema", KEPT)]);
        let base = landed(&[("schema/kept/shelf-v1.schema", KEPT)]);
        let report = run(&root, &base, &options()).expect("written sites");
        assert_files(&root, &[("patches/0001.patch", TEXT)]);
        assert_eq!(
            report.asked,
            [
                "patch 0001, shelf: add Book.state State ?",
                "patch 0001, shelf: add Book.count u32 ?",
                "patch 0001, shelf: written Book.label",
                "patch 0001, shelf: written Book.key",
                "patch 0001, shelf: written Book.sealed",
                "patch 0001, shelf: written Book.held",
            ]
        );
        write_files(&root, &[("patches/0001.patch", &filled)]);
        let report = run(&root, &base, &options()).expect("filled values");
        assert_files(&root, &[("patches/0001.patch", &filled)]);
        assert_eq!(report.findings, [], "the sites and the two values plan");
        end(&root);
    }

    #[test]
    fn an_added_field_that_takes_no_value_is_given_by_its_declarations_site() {
        const GROWN: &str = "family shelf 2\nrecord Book { id: bool, key: secret 16 }\nenum State { ready }\n";
        const TEXT: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nretype Book.id u8 bool\nadd Book.key secret 16 written\nwritten Book\n";
        let root =
            crate_files("declaration-site", &[("schema/shelf-v2.schema", GROWN), ("schema/kept/shelf-v1.schema", V1)]);
        let report = run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("one site");
        assert_files(&root, &[("patches/0001.patch", TEXT)]);
        assert_eq!(report.asked, ["patch 0001, shelf: written Book"]);
        assert_eq!(report.findings, [], "a declaration that is a site has no field site (11.3)");
        end(&root);
    }

    #[test]
    fn written_change_writes_its_skeleton_context_and_module_once() {
        const WRITTEN: &str = "family shelf 2\nrecord Book { id: bool }\nenum State { ready }\n";
        const SKELETON: &str = r"//! Patch 0001: the command wrote the skeleton; its functions are the author's (codec.md, 11.5).

use super::Context;
use skein_codec::patch::{Build, Halt, Old, Site};
use skein_codec::value::Node;

/// Site 0, `written Book`, shelf 1 to 2.
///
/// - `retype Book.id u8 bool`
pub fn book(_old: &Old, _new: &mut Build, _at: Node, _cx: &mut Context) -> Result<(), Halt> {
    Err(Halt::NoSite(Site::new(1, 0)))
}
";
        const CONTEXT: &str = "//! The owner's per-record inputs to written patch functions (codec.md, 11.5): the command wrote it once; it is the owner's.\n\n#[derive(Debug, Default)]\npub struct Context {}\n";
        let root =
            crate_files("generated", &[("schema/shelf-v2.schema", WRITTEN), ("schema/kept/shelf-v1.schema", V1)]);
        let base = landed(&[("schema/kept/shelf-v1.schema", V1)]);
        run(&root, &base, &options()).expect("written patch");
        assert_files(&root, &[("patches/p0001.rs", SKELETON), ("patches/context.rs", CONTEXT)]);
        assert_eq!(current(&root), Ok(()), "mod.rs as generated");
        assert!(!root.join("patches/golden").exists(), "a patch with sites waits for its functions");
        let authored = "//! Patch 0001, written.\npub fn book() {}\n";
        write_files(
            &root,
            &[("patches/p0001.rs", authored), ("patches/context.rs", "pub struct Context { pub n: u8 }\n")],
        );
        run(&root, &base, &options()).expect("run again");
        assert_files(
            &root,
            &[("patches/p0001.rs", authored), ("patches/context.rs", "pub struct Context { pub n: u8 }\n")],
        );
        end(&root);
    }

    #[test]
    fn a_patch_with_no_site_has_its_goldens_written_once_it_plans() {
        let root = crate_files("goldens", &[("schema/shelf-v2.schema", V2), ("schema/kept/shelf-v1.schema", V1)]);
        let base = landed(&[("schema/kept/shelf-v1.schema", V1)]);
        run(&root, &base, &options()).expect("asked patch");
        assert!(!root.join("patches/golden").exists(), "a `?` left: nothing planned, nothing written");
        write_files(&root, &[("patches/0001.patch", FILLED)]);
        let report = run(&root, &base, &options()).expect("filled patch");
        assert_eq!(report.findings, []);
        let mut names: Vec<_> = fs::read_dir(root.join("patches/golden/0001/shelf"))
            .expect("goldens written")
            .map(|entry| entry.expect("entry").file_name().into_string().expect("name"))
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "enum_state_ready_full.bin",
                "enum_state_ready_smallest.bin",
                "record_book_full.bin",
                "record_book_smallest.bin"
            ]
        );
        assert_eq!(
            golden(&root, 1, unwritten, Mode::Check),
            Ok(()),
            "the golden test passes on what the command wrote"
        );
        end(&root);
    }

    #[test]
    fn a_golden_check_never_writes_and_a_golden_write_restores() {
        const GOLDEN: &str = "patches/golden/0001/shelf/record_book_full.bin";
        let root = crate_files("golden-modes", &[("schema/shelf-v2.schema", V2), ("schema/kept/shelf-v1.schema", V1)]);
        let base = landed(&[("schema/kept/shelf-v1.schema", V1)]);
        run(&root, &base, &options()).expect("asked patch");
        write_files(&root, &[("patches/0001.patch", FILLED)]);
        run(&root, &base, &options()).expect("goldens written");
        let bytes = fs::read(root.join(GOLDEN)).expect("written golden");
        fs::remove_file(root.join(GOLDEN)).expect("remove golden");
        let Err(Error::Golden(failures)) = golden(&root, 1, unwritten, Mode::Check) else {
            unreachable!("a missing golden fails the check")
        };
        assert!(matches!(failures.as_slice(), [failure] if failure.contains(GOLDEN)), "{failures:?}");
        assert!(!root.join(GOLDEN).exists(), "Check writes nothing, whatever SKEIN_PATCH_WRITE says");
        assert_eq!(golden(&root, 1, unwritten, Mode::Write), Ok(()));
        assert_eq!(fs::read(root.join(GOLDEN)).expect("rewritten golden"), bytes);
        assert_eq!(golden(&root, 1, unwritten, Mode::Check), Ok(()));
        end(&root);
    }

    fn unwritten(site: Site, _old: &Old, _new: &mut Build, _at: Node, _cx: &mut ()) -> Result<(), Halt> {
        Err(Halt::NoSite(site))
    }

    #[test]
    fn deriving_again_keeps_values_sites_refusals_marks_and_after() {
        const AUTHOR: &str = "patch 1\nowner shop 1 after upstream 8\nmark reviewed\n\nfamily shelf 1 to 2\nadd Book.score u32 written\nwritten Book.score\nrefuses missing \"no score\"\n";
        const EDITED: &str =
            "family shelf 2\nrecord Book { id: u8, score: u32, note: option text 8 }\nenum State { ready }\n";
        const EXPECTED: &str = "patch 1\nowner shop 1 after upstream 8\nmark reviewed\n\nfamily shelf 1 to 2\nadd Book.score u32 written\nadd Book.note option text 8 none\nwritten Book.score\nrefuses missing \"no score\"\n";
        let root = crate_files(
            "authors",
            &[
                ("schema/shelf-v2.schema", EDITED),
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", V2),
                ("patches/0001.patch", AUTHOR),
            ],
        );
        let report =
            run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &Options::default()).expect("derive again");
        assert_files(&root, &[("patches/0001.patch", EXPECTED)]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn identical_statements_preserve_comments_byte_for_byte() {
        const COMMENTED: &str =
            "# reviewed\npatch 1\nowner shop 1\n\nfamily shelf 1 to 2 # shelf\nadd Book.score u32 7 # score\n";
        let root = crate_files(
            "comments",
            &[
                ("schema/shelf-v2.schema", V2),
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", V2),
                ("patches/0001.patch", COMMENTED),
            ],
        );
        let report = run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("same statements");
        assert_files(&root, &[("patches/0001.patch", COMMENTED)]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn removed_last_block_removes_patch_goldens_and_branch_kept() {
        let root = crate_files(
            "gone",
            &[
                ("schema/shelf-v1.schema", V1),
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", V2),
                ("patches/0001.patch", FILLED),
                ("patches/p0001.rs", "author\n"),
                ("patches/golden/0001/shelf/book", "golden"),
            ],
        );
        let report = run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("block gone");
        assert_eq!(report.patch, None);
        assert_eq!(report.asked, ["remove patches/p0001.rs: patch has no block"]);
        for path in ["patches/0001.patch", "schema/kept/shelf-v2.schema", "patches/golden/0001"] {
            assert!(!root.join(path).exists(), "{path} removed");
        }
        assert_files(&root, &[("patches/p0001.rs", "author\n")]);
        end(&root);
    }

    struct RefusalCase {
        name: &'static str,
        current: &'static str,
        verdict: Version,
    }

    #[test]
    fn version_refusals_preserve_every_family_file() {
        for case in [
            RefusalCase {
                name: "unmoved",
                current: "family shelf 1\nrecord Book { id: u16 }\nenum State { ready }\n",
                verdict: Version::Unmoved,
            },
            RefusalCase {
                name: "needless",
                current: "family shelf 2\nrecord Book { id: u8 }\nenum State { ready }\n",
                verdict: Version::Needless,
            },
            RefusalCase {
                name: "wrong",
                current: "family shelf 9\nrecord Book { id: u8 }\nenum State { ready }\n",
                verdict: Version::Wrong,
            },
        ] {
            let files = [("schema/shelf-v9.schema", case.current), ("schema/kept/shelf-v1.schema", V1)];
            let root = crate_files(case.name, &files);
            let report =
                run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("family refused");
            assert_eq!(report.outcomes, [Outcome::Refused { family: "shelf".into(), verdict: case.verdict }]);
            assert_files(&root, &files);
            end(&root);
        }
    }

    #[test]
    fn frozen_variant_growth_requires_the_next_version() {
        const GROWN: &str = "family shelf 2\nrecord Book { id: u8, note: option text 8 }\nenum State { ready, done }\n";
        let (root, base) = frozen_crate("frozen-growth", GROWN);
        let report = run(&root, &base, &options()).expect("unmoved");
        assert_eq!(report.outcomes, [Outcome::Refused { family: "shelf".into(), verdict: Version::Unmoved }]);
        assert_files(&root, &[("schema/kept/shelf-v2.schema", OPTIONAL)]);
        end(&root);
    }

    #[test]
    fn frozen_growth_at_next_version_writes_a_patch() {
        const GROWN: &str = "family shelf 3\nrecord Book { id: u8, note: option text 8 }\nenum State { ready, done }\n";
        const TEXT: &str = "patch 1\nowner shop 2\n\nfamily shelf 2 to 3\nadd State.done\n";
        let (root, base) = frozen_crate("frozen-next", OPTIONAL);
        write_files(&root, &[("schema/shelf-v3.schema", GROWN)]);
        let report = run(&root, &base, &options()).expect("moved");
        assert_eq!(report.outcomes, [Outcome::Patched { family: "shelf".into(), from: 2, to: 3 }]);
        assert_files(&root, &[("patches/0002.patch", TEXT), ("schema/kept/shelf-v3.schema", GROWN)]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn first_patch_requires_owner_without_writing_files() {
        let root = crate_files("owner", &[("schema/shelf-v2.schema", V2), ("schema/kept/shelf-v1.schema", V1)]);
        assert_eq!(run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &Options::default()), Err(Error::Owner));
        assert!(!root.join("patches").exists(), "no partial writes");
        end(&root);
    }

    #[test]
    fn two_branch_patches_are_refused_before_writing() {
        let files = [
            ("schema/shelf-v2.schema", V2),
            ("schema/kept/shelf-v1.schema", V1),
            ("patches/0001.patch", PATCH),
            ("patches/0002.patch", "patch 1\nowner shop 2\n"),
        ];
        let root = crate_files("two", &files);
        assert_eq!(
            run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()),
            Err(Error::Unlanded { patches: vec![1, 2] })
        );
        assert_files(&root, &files);
        end(&root);
    }

    #[test]
    fn conflict_markers_refuse_before_any_file_is_written() {
        let files = [
            ("schema/shelf-v2.schema", V2),
            ("schema/kept/shelf-v1.schema", V1),
            ("patches/p0001.rs", "<<<<<<< ours\n=======\n>>>>>>> theirs\n"),
        ];
        let root = crate_files("conflict", &files);
        assert_eq!(
            run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()),
            Err(Error::Conflict { path: "patches/p0001.rs".into() })
        );
        assert_files(&root, &files);
        assert!(!root.join("patches/0001.patch").exists(), "no partial patch");
        end(&root);
    }

    #[test]
    fn landed_golden_and_missing_kept_text_are_written_back() {
        let (root, base) = frozen_crate("restored", OPTIONAL);
        let mut files: BTreeMap<_, _> =
            base.paths().map(|path| (path.to_path_buf(), base.get(path).expect("landed path").to_vec())).collect();
        files.insert("patches/golden/0001/shelf/book".into(), b"landed".to_vec());
        fs::remove_file(root.join("schema/kept/shelf-v1.schema")).expect("remove old kept");
        write_files(&root, &[("patches/golden/0001/shelf/book", "edited")]);
        let report = run(&root, &Landed::new(files), &options()).expect("restored history");
        assert_eq!(
            report.restored,
            [PathBuf::from("patches/golden/0001/shelf/book"), PathBuf::from("schema/kept/shelf-v1.schema")]
        );
        assert_files(&root, &[("schema/kept/shelf-v1.schema", V1), ("patches/golden/0001/shelf/book", "landed")]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn proposal_rounds_find_renamed_referenced_declarations() {
        const OLD: &str = "family shelf 1\nrecord A { id: u8 }\nrecord B { a: A }\n";
        const NEW: &str = "family shelf 1\nrecord X { id: u8 }\nrecord Y { a: X }\n";
        let root = crate_files("rounds", &[("schema/shelf-v1.schema", NEW), ("schema/kept/shelf-v1.schema", OLD)]);
        let report =
            run(&root, &landed(&[("schema/kept/shelf-v1.schema", OLD)]), &options()).expect("proposals in rounds");
        assert_eq!(
            report.proposed,
            [
                ("shelf".into(), Rename::Declaration { old: "A".into(), new: "X".into() }),
                ("shelf".into(), Rename::Declaration { old: "B".into(), new: "Y".into() })
            ]
        );
        assert!(report.to_string().contains("--rename shelf B Y\n"), "confirmation command printed");
        assert_files(&root, &[("schema/kept/shelf-v1.schema", NEW)]);
        end(&root);
    }

    const BRANCH3: &str =
        "family shelf 3\nrecord Book { id: u8, note: option text 8, score: u32 }\nenum State { ready }\n";
    const BRANCH2: &str =
        "patch 1\nowner shop 2\n\nfamily shelf 2 to 3\nadd Book.score u32 written\nwritten Book.score\n";
    const MOVED3: &str =
        "patch 1\nowner shop 3\n\nfamily shelf 2 to 3\nadd Book.score u32 written\nwritten Book.score\n";
    const LABEL5: &str = "family label 5\nrecord Label { id: u8, note: option u8 }\n";
    const LANDED2: &str = "patch 1\nowner shop 2\n\nfamily label 4 to 5\nadd Label.note option u8 none\n";

    #[test]
    fn taken_number_moves_author_code_and_goldens_then_restores_history() {
        let base = landed(&[
            ("schema/kept/shelf-v1.schema", V1),
            ("schema/kept/shelf-v2.schema", OPTIONAL),
            ("schema/kept/label-v4.schema", LABEL),
            ("schema/kept/label-v5.schema", LABEL5),
            ("patches/0001.patch", FROZEN),
            ("patches/0002.patch", LANDED2),
            ("patches/p0002.rs", "landed code\n"),
            ("patches/golden/0002/label/label", "landed golden"),
        ]);
        let root = crate_files(
            "renumber",
            &[
                ("schema/shelf-v3.schema", BRANCH3),
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", OPTIONAL),
                ("schema/kept/shelf-v3.schema", BRANCH3),
                ("patches/0001.patch", FROZEN),
                ("patches/0002.patch", BRANCH2),
                ("patches/p0002.rs", "pub fn book_score() { p0002; Site::new(2, 0); kept::shelf::v2::book::ID }\n"),
                ("patches/golden/0002/shelf/book", "branch golden"),
            ],
        );
        let report = run(&root, &base, &Options::default()).expect("renumbered");
        assert_files(
            &root,
            &[
                ("patches/0002.patch", LANDED2),
                ("patches/0003.patch", MOVED3),
                ("patches/p0002.rs", "landed code\n"),
                ("patches/p0003.rs", "pub fn book_score() { p0003; Site::new(3, 0); kept::shelf::v2::book::ID }\n"),
                ("patches/golden/0002/label/label", "landed golden"),
                ("patches/golden/0003/shelf/book", "branch golden"),
            ],
        );
        assert_eq!(report.moved, Some(2));
        assert_eq!(
            report.restored,
            [
                PathBuf::from("patches/0002.patch"),
                PathBuf::from("patches/golden/0002/label/label"),
                PathBuf::from("patches/p0002.rs"),
                PathBuf::from("schema/kept/label-v4.schema"),
                PathBuf::from("schema/kept/label-v5.schema")
            ]
        );
        assert_eq!(report.findings, []);
        assert!(report.to_string().contains("patch 0003 (moved from 0002)"), "move printed");
        end(&root);
    }

    const BASE3: &str =
        "family shelf 3\nrecord Book { id: u8, note: option text 8, tag: option u8 }\nenum State { ready }\n";
    const BASE_PATCH: &str = "patch 1\nowner shop 2\n\nfamily shelf 2 to 3\nadd Book.tag option u8 none\n";
    const CURRENT4: &str = "family shelf 4\nrecord Book { id: u8, note: option text 8, tag: option u8, score: u32 }\nenum State { ready }\n";
    const AUTHOR: &str = "patch 1\nowner shop 2\nmark score\n\nfamily shelf 2 to 3\nadd Book.score u32 written\nwritten Book.score\nrefuses missing \"no score\"\n";
    const EXPECTED: &str = "patch 1\nowner shop 3\nmark score\n\nfamily shelf 3 to 4\nadd Book.score u32 written\nwritten Book.score\nrefuses missing \"no score\"\n";

    fn moved_family_crate(name: &str, author: &str) -> (PathBuf, Landed) {
        let base = landed(&[
            ("schema/kept/shelf-v1.schema", V1),
            ("schema/kept/shelf-v2.schema", OPTIONAL),
            ("schema/kept/shelf-v3.schema", BASE3),
            ("patches/0001.patch", FROZEN),
            ("patches/0002.patch", BASE_PATCH),
            ("patches/p0002.rs", "landed code\n"),
            ("patches/golden/0002/shelf/book", "landed golden"),
        ]);
        let root = crate_files(
            name,
            &[
                ("schema/shelf-v4.schema", CURRENT4),
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", OPTIONAL),
                ("schema/kept/shelf-v3.schema", BRANCH3),
                ("patches/0001.patch", FROZEN),
                ("patches/0002.patch", AUTHOR),
                ("patches/p0002.rs", author),
                ("patches/golden/0002/shelf/book", "branch golden"),
            ],
        );
        (root, base)
    }

    #[test]
    fn renumbered_alias_paths_follow_the_number_and_the_block() {
        let author =
            "use super::kept::shelf::p0002::{new, old};\npub fn book_score() { old::book::ID; new::book::SCORE; }\n";
        let moved =
            "use super::kept::shelf::p0003::{new, old};\npub fn book_score() { old::book::ID; new::book::SCORE; }\n";
        let (root, base) = moved_family_crate("aliases", author);
        run(&root, &base, &Options::default()).expect("aliases follow the moved family");
        assert_files(&root, &[("patches/p0003.rs", moved), ("patches/0003.patch", EXPECTED)]);
        let module = fs::read_to_string(root.join("patches/mod.rs")).expect("generated module");
        assert!(module.contains(concat!(
            "        pub mod p0003 {\n",
            "            pub use super::v3 as old;\n",
            "            pub use super::v4 as new;\n",
            "        }\n",
        )));
        end(&root);
    }

    #[test]
    fn family_moved_meanwhile_rederives_and_shifts_both_version_paths() {
        for (name, author, moved) in [
            (
                "qualified",
                "pub fn book_score() { p0002 Site::new(2, 0) kept::shelf::v2::book::ID shelf::v3::book::SCORE }\n",
                "pub fn book_score() { p0003 Site::new(3, 0) kept::shelf::v3::book::ID shelf::v4::book::SCORE }\n",
            ),
            (
                "group",
                "use super::kept::shelf::{v2, v3};\npub fn book_score() { p0002; Site::new(2, 0); v2::book::ID; v3::book::SCORE; }\n",
                "use super::kept::shelf::{v3, v4};\npub fn book_score() { p0003; Site::new(3, 0); v3::book::ID; v4::book::SCORE; }\n",
            ),
            (
                "ambiguous",
                "use super::kept::shelf::{v2, v3};\nuse super::kept::label::v4;\npub fn book_score() { p0002; Site::new(2, 0); v2::book::ID; v3::book::SCORE; shelf::v2::book::ID; }\n",
                "use super::kept::shelf::{v3, v4};\nuse super::kept::label::v4;\npub fn book_score() { p0003; Site::new(3, 0); v2::book::ID; v3::book::SCORE; shelf::v3::book::ID; }\n",
            ),
            (
                "single-import",
                "use super::kept::shelf::v2;\npub fn book_score() { p0002; Site::new(2, 0); v2::book::ID; shelf::v20::book::ID; bookshelf::v2::book::ID; /* shelf::v2::book::ID */ \"shelf::v2::book::ID\"; }\n",
                "use super::kept::shelf::v3;\npub fn book_score() { p0003; Site::new(3, 0); v3::book::ID; shelf::v20::book::ID; bookshelf::v2::book::ID; /* shelf::v2::book::ID */ \"shelf::v2::book::ID\"; }\n",
            ),
        ] {
            let (root, base) = moved_family_crate(name, author);
            let report = run(&root, &base, &Options::default()).expect("family moved meanwhile");
            assert_files(
                &root,
                &[
                    ("patches/0003.patch", EXPECTED),
                    ("schema/kept/shelf-v3.schema", BASE3),
                    ("schema/kept/shelf-v4.schema", CURRENT4),
                    ("patches/p0003.rs", moved),
                    ("patches/golden/0002/shelf/book", "landed golden"),
                ],
            );
            assert!(!root.join("patches/golden/0003").exists(), "old-version goldens dropped");
            assert_eq!(report.moved, Some(2));
            assert_eq!(
                report.asked,
                [
                    "patch 0003, shelf: written Book.score",
                    "review patches/p0003.rs: shelf now moves 3 to 4",
                    "patches/golden/0002/shelf/book: goldens dropped; shelf's old kept version moved"
                ]
            );
            assert_eq!(report.findings, []);
            end(&root);
        }
    }

    #[test]
    fn stray_landed_patch_edit_is_rederived_without_extra_blocks() {
        let (root, base) = frozen_crate("stray", OPTIONAL);
        write_files(
            &root,
            &[(
                "patches/0001.patch",
                "patch 1\nowner shop 1\nmark stray\n\nfamily shelf 1 to 2\nadd Book.note option text 8 none\n",
            )],
        );
        let report = run(&root, &base, &options()).expect("stray edit undone");
        assert_files(&root, &[("patches/0001.patch", FROZEN)]);
        assert!(!root.join("patches/0002.patch").exists(), "no diff needs a block");
        assert_eq!(report.restored, [PathBuf::from("patches/0001.patch")]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn changed_added_field_type_discards_the_authors_old_value() {
        const NEW: &str = "family shelf 2\nrecord Book { id: u8, score: u16 }\nenum State { ready }\n";
        const EXPECTED: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nadd Book.score u16 ?\n";
        let root = crate_files(
            "changed-type",
            &[
                ("schema/shelf-v2.schema", NEW),
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", V2),
                ("patches/0001.patch", FILLED),
            ],
        );
        let report =
            run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("different added type");
        assert_files(&root, &[("patches/0001.patch", EXPECTED)]);
        assert_eq!(report.asked, ["patch 0001, shelf: add Book.score u16 ?"]);
        end(&root);
    }

    #[test]
    fn author_field_site_supplies_retype_without_declaration_site() {
        const NEW: &str = "family shelf 2\nrecord Book { id: bool }\nenum State { ready }\n";
        const AUTHOR: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nretype Book.id u8 bool\nwritten Book.id\nrefuses missing \"no value\"\n";
        let root = crate_files(
            "field-site",
            &[
                ("schema/shelf-v2.schema", NEW),
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", NEW),
                ("patches/0001.patch", AUTHOR),
            ],
        );
        let report =
            run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("field site preserved");
        assert_files(&root, &[("patches/0001.patch", AUTHOR)]);
        assert_eq!(report.asked, ["patch 0001, shelf: written Book.id"]);
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn gone_field_removes_its_site_and_refusals() {
        const AUTHOR: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nadd Book.score u32 written\nwritten Book.score\nrefuses missing \"no score\"\n";
        let root = crate_files(
            "gone-site",
            &[
                ("schema/shelf-v2.schema", OPTIONAL),
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", V2),
                ("patches/0001.patch", AUTHOR),
            ],
        );
        let report = run(&root, &landed(&[("schema/kept/shelf-v1.schema", V1)]), &options()).expect("site gone");
        assert_files(&root, &[("patches/0001.patch", FROZEN)]);
        assert_eq!(report.asked, Vec::<String>::new());
        assert_eq!(report.findings, []);
        end(&root);
    }

    #[test]
    fn wrong_v9_over_kept_v7_refuses_without_gap_writes() {
        const OLD: &str = "family shelf 7\nrecord Book { id: u8 }\nenum State { ready }\n";
        const NEW: &str = "family shelf 9\nrecord Book { id: u8 }\nenum State { ready }\n";
        let files = [("schema/shelf-v9.schema", NEW), ("schema/kept/shelf-v7.schema", OLD)];
        let root = crate_files("wrong-nine", &files);
        let report = run(&root, &landed(&[("schema/kept/shelf-v7.schema", OLD)]), &options()).expect("wrong version");
        assert_eq!(report.outcomes, [Outcome::Refused { family: "shelf".into(), verdict: Version::Wrong }]);
        assert_files(&root, &files);
        assert!(!root.join("schema/kept/shelf-v8.schema").exists(), "no invented kept version");
        end(&root);
    }

    #[test]
    fn report_displays_outcomes_requests_proposals_and_findings() {
        use skein_codegen::patch::{Finding, Report};
        let report = Report {
            outcomes: vec![
                Outcome::Current { family: "label".into() },
                Outcome::Patched { family: "shelf".into(), from: 1, to: 2 },
            ],
            patch: Some(3),
            moved: Some(2),
            restored: vec!["patches/0002.patch".into()],
            asked: vec!["patch 0003, shelf: written Book".into()],
            proposed: vec![
                ("shelf".into(), Rename::Declaration { old: "Old".into(), new: "Book".into() }),
                ("shelf".into(), Rename::Part { decl: "Book".into(), old: "id".into(), new: "key".into() }),
            ],
            findings: vec![Finding::PatchGap { missing: 1 }],
        };
        assert_eq!(
            report.to_string(),
            "label: current\nshelf: patched 1 to 2\npatch 0003 (moved from 0002)\nrestored patches/0002.patch\nasked: patch 0003, shelf: written Book\n--rename shelf Old Book\n--rename shelf Book.id key\npatch 0001 missing\n"
        );
    }

    #[test]
    fn refreshed_rename_survives_rerun_and_the_next_patch() {
        const NEXT: &str = "family shelf 2\nrecord Book { key: u8, note: option text 8 }\nenum State { ready }\n";
        const EXPECTED: &str =
            "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nrename Book.id key\nadd Book.note option text 8 none\n";
        let root = crate_files(
            "refresh-rerun",
            &[("schema/shelf-v1.schema", RENAMED), ("schema/kept/shelf-v1.schema", RENAMED)],
        );
        let base = landed(&[("schema/kept/shelf-v1.schema", V1)]);
        let report = run(&root, &base, &options()).expect("already refreshed");
        assert_eq!(report.outcomes, [Outcome::Current { family: "shelf".into() }]);
        assert_eq!(report.proposed, []);
        write_files(&root, &[("schema/shelf-v2.schema", NEXT)]);
        let report = run(&root, &base, &options()).expect("patch retains confirmed pairing");
        assert_files(
            &root,
            &[
                ("schema/kept/shelf-v1.schema", V1),
                ("schema/kept/shelf-v2.schema", NEXT),
                ("patches/0001.patch", EXPECTED),
            ],
        );
        assert_eq!(report.findings, []);
        end(&root);
    }
}
