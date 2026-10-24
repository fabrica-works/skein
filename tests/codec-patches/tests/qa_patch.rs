//! Black-box promises of codec.md 12, using only the owner's public surface.

#[cfg(test)]
extern crate alloc;

#[cfg(test)]
mod tests {

    use alloc::collections::BTreeMap;
    use core::sync::atomic::{AtomicU64, Ordering};
    use std::fs;
    use std::path::{Path, PathBuf};

    use skein_codec::patch::{Build, Halt, Old, RefusalIx, Site};
    use skein_codec::value::Node;
    use skein_codegen::diff::{Rename, Version};
    use skein_codegen::patch::{self, Error, Finding, Landed, Options, Outcome, Report};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            loop {
                let serial = NEXT.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!("skein-qa-patch-{}-{serial}", std::process::id()));
                let created = fs::create_dir(&path);
                if created.as_ref().is_err_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) {
                    continue;
                }
                created.expect("create scratch owner");
                return Self(path);
            }
        }

        fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().expect("parent")).expect("directories");
            fs::write(path, bytes).expect("write scratch file");
        }

        fn text(&self, path: &str) -> String {
            fs::read_to_string(self.0.join(path)).expect("scratch text")
        }

        fn remove(&self, path: &str) {
            fs::remove_file(self.0.join(path)).expect("remove scratch file");
        }

        fn schema(&self, family: &str, version: u16, body: &str) {
            self.write(&format!("schema/{family}-v{version}.schema"), format!("family {family} {version}\n{body}\n"));
        }

        fn run(&self, landed: &Landed, options: &Options) -> Report {
            patch::run(&self.0, landed, options).expect("run patch command")
        }

        fn landed(&self) -> Landed {
            Landed::read(&self.0).expect("read landed files")
        }

        fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
            fn visit(root: &Path, at: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
                for entry in fs::read_dir(at).expect("read scratch directory") {
                    let path = entry.expect("entry").path();
                    if path.is_dir() {
                        visit(root, &path, files);
                    } else {
                        files.insert(
                            path.strip_prefix(root).expect("relative path").to_owned(),
                            fs::read(path).expect("bytes"),
                        );
                    }
                }
            }
            let mut files = BTreeMap::new();
            visit(&self.0, &self.0, &mut files);
            files
        }

        fn copy(&self) -> Self {
            let other = Self::new();
            for (path, bytes) in self.snapshot() {
                other.write(path.to_str().expect("UTF-8 fixture path"), bytes);
            }
            other
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove scratch owner");
        }
    }

    fn options() -> Options {
        Options { owner: Some("qa".into()), ..Options::default() }
    }

    fn baseline() -> (Scratch, Landed) {
        let owner = Scratch::new();
        owner.schema("alpha", 7, "record R { x: u8 }");
        owner.run(&Landed::none(), &Options { baseline: true, ..Options::default() });
        let landed = owner.landed();
        (owner, landed)
    }

    fn frozen() -> (Scratch, Landed) {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: u16 }");
        let report = owner.run(&landed, &options());
        assert!(report.findings.is_empty(), "{report:?}");
        let landed = owner.landed();
        (owner, landed)
    }

    fn no_site(site: Site, _old: &Old, _new: &mut Build, _at: Node, _cx: &mut ()) -> Result<(), Halt> {
        Err(Halt::NoSite(site))
    }

    fn fixture() -> Scratch {
        fn copy(root: &Path, at: &Path, owner: &Scratch) {
            for entry in fs::read_dir(at).expect("fixture directory") {
                let path = entry.expect("fixture entry").path();
                if path.is_dir() {
                    copy(root, &path, owner);
                } else {
                    owner.write(
                        path.strip_prefix(root).expect("fixture relative").to_str().expect("UTF-8"),
                        fs::read(&path).expect("fixture bytes"),
                    );
                }
            }
        }
        let owner = Scratch::new();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        copy(root, &root.join("schema"), &owner);
        copy(root, &root.join("patches"), &owner);
        owner
    }

    #[test]
    fn s12_1_baseline_guard_then_keep_greatest_current_in_family_order() {
        let owner = Scratch::new();
        owner.schema("zeta", 3, "record Z { x: u8 }");
        owner.schema("alpha", 7, "record A { x: u8 }");
        owner.schema("alpha", 8, "record A { x: u16 }");
        let before = owner.snapshot();
        assert_eq!(patch::run(&owner.0, &Landed::none(), &Options::default()), Err(Error::Baseline));
        assert_eq!(owner.snapshot(), before);
        let report = owner.run(&Landed::none(), &Options { baseline: true, ..Options::default() });
        assert_eq!(
            report.outcomes,
            vec![
                Outcome::Kept { family: "alpha".into(), version: 8 },
                Outcome::Kept { family: "zeta".into(), version: 3 }
            ]
        );
        assert_eq!(report.patch, None);
        assert!(report.findings.is_empty(), "{report:?}");
        assert!(!owner.0.join("schema/kept/alpha-v7.schema").exists());
        assert_eq!(owner.text("schema/kept/alpha-v8.schema"), owner.text("schema/alpha-v8.schema"));
        assert_eq!(patch::current(&owner.0), Ok(()));
    }

    #[test]
    fn s12_1_unlanded_family_is_kept_afresh_without_baseline_flag() {
        let (owner, landed) = baseline();
        owner.schema("beta", 4, "record B { x: u8 }");
        owner.run(&landed, &Options::default());
        owner.schema("beta", 5, "record B { x: bool }");
        let report = owner.run(&landed, &Options::default());
        assert!(report.outcomes.contains(&Outcome::Kept { family: "beta".into(), version: 5 }), "{report:?}");
        assert_eq!(report.patch, None);
        assert!(!owner.0.join("schema/kept/beta-v4.schema").exists());
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn s12_2_unfrozen_refresh_is_allowed_but_check_does_not_write() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 7, "# review\nrecord R { x: u8 }\nrecord Extra { b: bool }");
        let before = owner.snapshot();
        assert_eq!(patch::check(&owner.0, &landed), Ok(vec![Finding::Lagging { family: "alpha".into(), version: 7 }]));
        assert_eq!(owner.snapshot(), before);
        let report = owner.run(&landed, &Options::default());
        assert_eq!(report.outcomes, vec![Outcome::Refreshed { family: "alpha".into(), version: 7 }]);
        assert_eq!(owner.text("schema/kept/alpha-v7.schema"), owner.text("schema/alpha-v7.schema"));
        assert!(report.findings.is_empty(), "{report:?}");
        assert_eq!(patch::check(&owner.0, &landed), Ok(vec![]));
    }

    #[test]
    fn s12_2_frozen_append_requires_next_version() {
        let (owner, landed) = frozen();
        owner.schema("alpha", 8, "record R { x: u16 }\nenum Added { first }");
        let before = owner.text("schema/kept/alpha-v8.schema");
        let report = owner.run(&landed, &Options::default());
        assert_eq!(report.outcomes, vec![Outcome::Refused { family: "alpha".into(), verdict: Version::Unmoved }]);
        assert_eq!(owner.text("schema/kept/alpha-v8.schema"), before);
        owner.schema("alpha", 9, "record R { x: u16 }\nenum Added { first }");
        let report = owner.run(&landed, &Options::default());
        assert_eq!(report.patch, Some(2));
        assert!(owner.text("patches/0002.patch").contains("add Added\n"));
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn s12_3_first_patch_requires_owner_and_keeps_after_order() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: u16 }");
        assert_eq!(patch::run(&owner.0, &landed, &Options::default()), Err(Error::Owner));
        let report = owner.run(&landed, &Options { after: vec![("z".into(), 9), ("a".into(), 2)], ..options() });
        assert_eq!(report.patch, Some(1));
        assert_eq!(
            owner.text("patches/0001.patch"),
            "patch 1\nowner qa 1 after z 9 a 2\n\nfamily alpha 7 to 8\nwiden R.x u8 u16\n"
        );
        let landed = owner.landed();
        owner.schema("alpha", 9, "record R { x: u32 }");
        owner.run(&landed, &Options::default());
        assert!(owner.text("patches/0002.patch").contains("owner qa 2\n"));
    }

    #[test]
    fn s12_3_rederive_keeps_values_sites_refusals_marks_and_comments() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: u8, score: u16 }");
        let report = owner.run(&landed, &options());
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f, Finding::Unplanned { patch: 1, family, .. } if family == "alpha")),
            "{report:?}"
        );
        let authored = "# reviewed\npatch 1\nowner qa 1\nmark review\n\nfamily alpha 7 to 8\nadd R.score u16 13\nwritten R.x\nrefuses bad \"bad input\"\n";
        owner.write("patches/0001.patch", authored);
        owner.run(&landed, &Options::default());
        assert_eq!(owner.text("patches/0001.patch"), authored);
        owner.schema("alpha", 8, "record R { x: u8, score: u16, note: option text 8 }");
        let report = owner.run(&landed, &Options::default());
        let text = owner.text("patches/0001.patch");
        assert_eq!(
            text,
            "patch 1\nowner qa 1\nmark review\n\nfamily alpha 7 to 8\nadd R.score u16 13\nadd R.note option text 8 none\nwritten R.x\nrefuses bad \"bad input\"\n"
        );
        assert!(report.findings.is_empty(), "{report:?}");
        assert!(owner.text("patches/p0001.rs").contains("pub fn r_x("));
    }

    #[test]
    fn s12_3_changed_field_type_does_not_reuse_author_value() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: u8, score: u16 }");
        owner.run(&landed, &options());
        owner.write("patches/0001.patch", owner.text("patches/0001.patch").replace("u16 ?", "u16 13"));
        owner.schema("alpha", 8, "record R { x: u8, score: bool }");
        let report = owner.run(&landed, &Options::default());
        assert!(owner.text("patches/0001.patch").contains("add R.score bool ?\n"));
        assert!(!report.asked.is_empty(), "{report:?}");
    }

    #[test]
    fn s12_3_no_value_types_get_field_sites_instead_of_questions() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record Child { n: u8 }\nenum Choice { only: Child }\nrecord R { x: u8, token: secret 8, child: Child, choice: Choice, held: bytes 32 holds remote Item }");
        let report = owner.run(&landed, &options());
        let text = owner.text("patches/0001.patch");
        for field in ["token", "child", "choice", "held"] {
            assert!(text.contains(&format!("written R.{field}\n")), "{text}");
            assert!(!text.contains(&format!("R.{field} secret 8 ?")), "{text}");
            assert!(owner.text("patches/p0001.rs").contains(&format!("pub fn r_{field}(")));
        }
        assert!(!text.contains('?'), "{text}");
        assert!(report.findings.is_empty(), "{report:?}");
        let skeleton = owner.text("patches/p0001.rs");
        assert!(skeleton.contains("Err(Halt::NoSite(Site::new(1, 0)))"), "{skeleton}");
        assert!(!skeleton.contains("panic!"));
    }

    #[test]
    fn s12_3_and_s11_3_declaration_site_supplies_added_secret_without_field_site() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: bool, token: secret 8 }");
        let report = owner.run(&landed, &options());
        // 11.3: "A declaration that is a site has no field site" also governs 12.3's added fields.
        assert!(report.findings.is_empty(), "{report:?}");
        let text = owner.text("patches/0001.patch");
        assert!(text.contains("written R\n"), "{text}");
        assert!(!text.contains("written R.token\n"), "{text}");
    }

    #[test]
    fn s12_3_skeleton_appends_missing_function_and_preserves_author_code() {
        let owner = Scratch::new();
        owner.schema("alpha", 7, "record R { x: u8 }\nenum Other { first, second }");
        owner.run(&Landed::none(), &Options { baseline: true, ..Options::default() });
        let landed = owner.landed();
        owner.schema("alpha", 8, "record R { x: bool }\nenum Other { first, second }");
        owner.run(&landed, &options());
        let authored = owner
            .text("patches/p0001.rs")
            .replace("Err(Halt::NoSite(Site::new(1, 0)))", "_new.open()?; _new.put_bool(true)?; _new.close()");
        owner.write("patches/p0001.rs", &authored);
        owner.write(
            "patches/context.rs",
            "// author's context\n#[derive(Default)] pub struct Context { pub value: u8 }\n",
        );
        owner.schema("alpha", 8, "record R { x: bool }\nenum Other { first }");
        owner.run(&landed, &Options::default());
        let appended = owner.text("patches/p0001.rs");
        assert!(appended.starts_with(&authored), "{appended}");
        assert!(appended.contains("pub fn other("), "{appended}");
        assert!(appended.contains("Err(Halt::NoSite(Site::new(1, 1)))"), "{appended}");
        assert!(owner.text("patches/context.rs").contains("pub value: u8"));
    }

    #[test]
    fn s12_2_cancelled_patch_removes_text_goldens_and_reports_author_module() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: bool }");
        owner.run(&landed, &options());
        owner.write("patches/golden/0001/alpha/review.bin", b"review");
        owner.remove("schema/alpha-v8.schema");
        let report = owner.run(&landed, &Options::default());
        assert_eq!(report.patch, None);
        assert!(!owner.0.join("patches/0001.patch").exists());
        assert!(!owner.0.join("patches/golden/0001").exists());
        assert!(owner.0.join("patches/p0001.rs").exists());
        assert!(report.asked.iter().any(|s| s.contains("p0001.rs")), "{report:?}");
    }

    #[test]
    fn s12_4_frozen_rename_waits_then_is_consumed_by_patch() {
        let (owner, landed) = frozen();
        owner.schema("alpha", 8, "record R { y: u16 }");
        let rename = Rename::Part { decl: "R".into(), old: "x".into(), new: "y".into() };
        let report =
            owner.run(&landed, &Options { renames: vec![("alpha".into(), rename.clone())], ..Options::default() });
        assert_eq!(report.outcomes, vec![Outcome::Waiting { family: "alpha".into(), renames: vec![rename] }]);
        assert_eq!(owner.text("schema/kept/alpha.renames"), "rename R.x y\n");
        let report = owner.run(&landed, &Options::default());
        assert!(report.findings.is_empty(), "{report:?}");
        owner.schema("alpha", 9, "record R { y: u32 }");
        let report = owner.run(&landed, &Options::default());
        assert!(owner.text("patches/0002.patch").contains("rename R.x y\nwiden R.y u16 u32\n"));
        assert!(!owner.0.join("schema/kept/alpha.renames").exists());
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn s12_2_proposals_rounds_do_not_confirm_renames_in_files() {
        let owner = Scratch::new();
        owner.schema("alpha", 1, "record Inner { x: u8 }\nrecord Outer { value: Inner }\nrecord Stable { n: u8 }");
        owner.run(&Landed::none(), &Options { baseline: true, ..Options::default() });
        let landed = owner.landed();
        owner.schema("alpha", 2, "record Core { x: u8 }\nrecord Shell { value: Core }\nrecord Stable { n: u16 }");
        let report = owner.run(&landed, &options());
        assert!(
            report
                .proposed
                .contains(&("alpha".into(), Rename::Declaration { old: "Inner".into(), new: "Core".into() })),
            "{report:?}"
        );
        assert!(
            report
                .proposed
                .contains(&("alpha".into(), Rename::Declaration { old: "Outer".into(), new: "Shell".into() })),
            "{report:?}"
        );
        // 12.2: no unconfirmed proposal reaches a file, including proposals after another round.
        assert!(!owner.text("patches/0001.patch").contains("rename "));
    }

    #[test]
    fn s12_5_all_refused_verdicts_keep_family_files() {
        for (version, body, verdict) in [
            (7, "record R { x: u16 }", Version::Unmoved),
            (8, "record R { x: u8 }", Version::Needless),
            (9, "record R { x: u16 }", Version::Wrong),
        ] {
            let (owner, landed) = baseline();
            owner.schema("alpha", version, body);
            let before = owner.snapshot();
            let report = owner.run(&landed, &options());
            assert_eq!(report.outcomes, vec![Outcome::Refused { family: "alpha".into(), verdict }]);
            assert_eq!(owner.snapshot(), before);
            assert!(
                report.findings.contains(&Finding::Version {
                    family: "alpha".into(),
                    verdict,
                    kept: 7,
                    current: version
                }),
                "{report:?}"
            );
        }
    }

    #[test]
    fn s12_2_family_refusal_does_not_block_other_families_patch() {
        let (owner, _) = baseline();
        owner.schema("beta", 1, "record B { x: u8 }");
        owner.run(&owner.landed(), &Options::default());
        let landed = owner.landed();
        owner.schema("alpha", 7, "record R { x: u16 }");
        owner.schema("beta", 2, "record B { x: u16 }");
        let report = owner.run(&landed, &options());
        assert_eq!(
            report.outcomes,
            vec![
                Outcome::Refused { family: "alpha".into(), verdict: Version::Unmoved },
                Outcome::Patched { family: "beta".into(), from: 1, to: 2 }
            ]
        );
        assert!(!owner.text("patches/0001.patch").contains("family alpha"));
        assert!(owner.text("patches/0001.patch").contains("family beta 1 to 2"));
    }

    #[test]
    fn s12_2_restores_missing_and_changed_landed_patch_kept_and_golden() {
        let (owner, landed) = frozen();
        owner.write("patches/0001.patch", "patch 1\nowner intruder 1\n\nfamily alpha 7 to 8\nwiden R.x u8 u16\n");
        owner.write("schema/kept/alpha-v8.schema", "family alpha 8\nrecord R { x: u32 }\n");
        owner.remove("patches/golden/0001/alpha/record_r_smallest.bin");
        let before = owner.snapshot();
        let findings = patch::check(&owner.0, &landed).expect("check readable edits");
        for path in
            ["patches/0001.patch", "schema/kept/alpha-v8.schema", "patches/golden/0001/alpha/record_r_smallest.bin"]
        {
            assert!(findings.contains(&Finding::Changed { path: path.into() }), "{findings:?}");
        }
        assert_eq!(owner.snapshot(), before);
        let report = owner.run(&landed, &Options::default());
        for path in
            ["patches/0001.patch", "schema/kept/alpha-v8.schema", "patches/golden/0001/alpha/record_r_smallest.bin"]
        {
            assert!(report.restored.contains(&PathBuf::from(path)), "{report:?}");
            assert_eq!(
                fs::read(owner.0.join(path)).expect("restored bytes"),
                landed.get(Path::new(path)).expect("landed bytes")
            );
        }
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn s12_6_renumber_restores_taken_number_and_preserves_written_code() {
        fn truth(_site: Site, _old: &Old, new: &mut Build, _at: Node, _cx: &mut ()) -> Result<(), Halt> {
            new.open()?;
            new.put_bool(true)?;
            new.close()
        }
        let (branch, base) = baseline();
        let upstream = branch.copy();
        branch.schema("alpha", 8, "record R { x: bool }");
        branch.run(&base, &options());
        let authored = branch
            .text("patches/p0001.rs")
            .replace("Err(Halt::NoSite(Site::new(1, 0)))", "_new.open()?; _new.put_bool(true)?; _new.close()");
        branch.write("patches/p0001.rs", format!("{authored}\nuse super::kept::alpha::v7;\nuse super::kept::alpha::v8;\n// author's code\nconst _: skein_codec::patch::Site = skein_codec::patch::Site::new(1, 0);\n"));
        for name in ["record_r_smallest.bin", "record_r_full.bin"] {
            branch.write(&format!("patches/golden/0001/alpha/{name}"), [1]);
        }
        assert_eq!(patch::goldens(&branch.0, 1, truth), Ok(()));
        upstream.schema("alpha", 8, "record R { x: u16 }");
        upstream.run(&base, &options());
        let landed = upstream.landed();
        branch.schema("alpha", 8, "record R { x: u16 }");
        branch.schema("alpha", 9, "record R { x: bool }");
        let report = branch.run(&landed, &Options::default());
        assert_eq!(report.moved, Some(1));
        assert_eq!(report.patch, Some(2));
        assert_eq!(branch.text("patches/0001.patch"), upstream.text("patches/0001.patch"));
        assert!(branch.text("patches/0002.patch").contains("family alpha 8 to 9\nretype R.x u16 bool\nwritten R\n"));
        let code = branch.text("patches/p0002.rs");
        assert!(code.contains("alpha::v8;") && code.contains("alpha::v9;"), "{code}");
        assert!(code.contains("Site::new(2, 0)"), "{code}");
        assert!(code.contains("// author's code"), "{code}");
        assert!(!branch.0.join("patches/p0001.rs").exists());
        // 12.6: a family moved meanwhile has its goldens dropped, not moved, and named in the report.
        assert!(!branch.0.join("patches/golden/0002/alpha").exists());
        assert!(
            report.asked.iter().any(|line| line.contains("goldens dropped; alpha's old kept version moved")),
            "{report:?}"
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn s12_6_conflict_markers_refused_with_their_path() {
        for path in ["schema/alpha-v7.schema", "schema/kept/alpha-v7.schema", "patches/mod.rs", "patches/context.rs"] {
            let (owner, landed) = baseline();
            owner.write(path, "<<<<<<< branch\nleft\n=======\nright\n>>>>>>> base\n");
            let result = patch::run(&owner.0, &landed, &Options::default());
            assert!(
                matches!(&result, Err(Error::Conflict { path: named }) if named.ends_with(path)),
                "{path}: {result:?}"
            );
        }
    }

    #[test]
    fn s12_8_two_branch_patches_are_refused() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: u16 }");
        owner.run(&landed, &options());
        owner.write("patches/0002.patch", "patch 1\nowner qa 2\n");
        assert_eq!(patch::run(&owner.0, &landed, &options()), Err(Error::Unlanded { patches: vec![1, 2] }));
    }

    #[test]
    fn s12_7_check_collects_gaps_number_and_unplanned_without_writing() {
        let owner = Scratch::new();
        owner.schema("alpha", 3, "record R { x: u32 }");
        owner.write("schema/kept/alpha-v1.schema", "family alpha 1\nrecord R { x: u8 }\n");
        owner.write("schema/kept/alpha-v3.schema", "family alpha 3\nrecord R { x: u32 }\n");
        owner.write("patches/0003.patch", "patch 1\nowner qa 4\n\nfamily alpha 1 to 2\nwiden R.x u8 u16\n");
        owner.schema("beta", 1, "record B { x: u8 }");
        let before = owner.snapshot();
        let findings = patch::check(&owner.0, &Landed::none()).expect("check malformed history");
        for expected in [
            Finding::KeptGap { family: "alpha".into(), missing: 2 },
            Finding::Unpatched { family: "alpha".into(), version: 3 },
            Finding::Unkept { family: "beta".into() },
            Finding::PatchGap { missing: 1 },
            Finding::PatchGap { missing: 2 },
            Finding::Misnumbered { patch: 3, level: 4 },
        ] {
            assert!(findings.contains(&expected), "missing {expected:?}: {findings:?}");
        }
        assert!(
            findings.iter().any(|f| matches!(f, Finding::Unplanned { patch: 3, family, .. } if family == "alpha")),
            "{findings:?}"
        );
        assert_eq!(owner.snapshot(), before);
    }

    #[test]
    fn s12_7_frozen_drop_comments_and_declaration_order_are_no_finding() {
        let owner = Scratch::new();
        owner.schema("alpha", 7, "record R { x: u8 }\nrecord Extra { x: bool }\nenum E { a, b }");
        owner.run(&Landed::none(), &Options { baseline: true, ..Options::default() });
        let base = owner.landed();
        owner.schema("alpha", 8, "record R { x: u16 }\nrecord Extra { x: bool }\nenum E { a, b }");
        owner.run(&base, &options());
        let landed = owner.landed();
        owner.schema("alpha", 8, "# the store removes Extra's roots\nenum E { a, b }\nrecord R { x: u16 }");
        assert_eq!(patch::check(&owner.0, &landed), Ok(vec![]));
        let kept = owner.text("schema/kept/alpha-v8.schema");
        owner.run(&landed, &Options::default());
        assert_eq!(owner.text("schema/kept/alpha-v8.schema"), kept);
    }

    #[test]
    fn s12_7_invalid_kept_filename_is_schema_error() {
        let owner = Scratch::new();
        owner.schema("alpha", 7, "record R { x: u8 }");
        owner.write("schema/kept/alpha-v8.schema", "family alpha 7\nrecord R { x: u8 }\n");
        let result = patch::check(&owner.0, &Landed::none());
        assert!(
            matches!(&result, Err(Error::Schema { path, .. }) if path.ends_with("schema/kept/alpha-v8.schema")),
            "{result:?}"
        );
    }

    #[test]
    fn s12_3_module_is_generated_from_kept_and_patches_alone() {
        let owner = fixture();
        let before = owner.snapshot();
        let generated = patch::module(&owner.0).expect("generate module");
        assert_eq!(generated, owner.text("patches/mod.rs"));
        assert_eq!(owner.snapshot(), before);
        owner.schema("unkept", 22, "record New { x: u8 }");
        owner.write("patches/context.rs", "// owner's context is not module input\n");
        owner.write("patches/p0003.rs", "// owner's function is not module input\n");
        assert_eq!(patch::module(&owner.0).expect("module independent of author code"), generated);
        owner.write("patches/mod.rs", "// stale\n");
        assert_eq!(patch::current(&owner.0), Err(Error::Stale));
        owner.write("patches/mod.rs", generated);
        assert_eq!(patch::current(&owner.0), Ok(()));
    }

    #[test]
    fn s12_3_each_capital_starts_a_snake_case_word() {
        let owner = Scratch::new();
        owner.schema("alpha", 1, "record HTTP { x: u8 }\nrecord Http { x: u8 }");
        // 12.3: each capital starts a word, so HTTP is `h_t_t_p` and Http is `http`, two modules.
        let result = patch::run(&owner.0, &Landed::none(), &Options { baseline: true, ..Options::default() });
        assert!(result.is_ok(), "{result:?}");
        let generated = patch::module(&owner.0).expect("generate module");
        assert!(generated.contains("pub mod h_t_t_p {") && generated.contains("pub mod http {"), "{generated}");
    }

    #[test]
    fn s12_3_constant_spelling_collision_is_refused() {
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: bool }");
        owner.run(&landed, &options());
        owner.write(
            "patches/0001.patch",
            format!("{}refuses bad-input \"one\"\nrefuses bad_input \"two\"\n", owner.text("patches/0001.patch")),
        );
        let result = patch::module(&owner.0);
        assert!(matches!(result, Err(Error::Spelled { .. })), "{result:?}");
    }

    #[test]
    fn s12_3_module_keeps_global_part_indices_and_keyword_names() {
        let owner = Scratch::new();
        owner.schema("type", 1, "record First { x: u8 }\nrecord Match { score: u16 }\nenum Kind { first, other }");
        owner.run(&Landed::none(), &Options { baseline: true, ..Options::default() });
        let text = patch::module(&owner.0).expect("keyword module");
        assert!(text.contains("pub mod r#type {"), "{text}");
        assert!(text.contains("pub mod r#match {"), "{text}");
        assert!(text.contains("pub const SCORE: FieldIx = FieldIx::new(1);"), "{text}");
        assert!(text.contains("pub const OTHER: VariantIx = VariantIx::new(1);"), "{text}");
    }

    #[test]
    fn s12_3_goldens_missing_different_and_stale_all_named_read_only() {
        let owner = fixture();
        owner.remove("patches/golden/0003/shelf/record_book_smallest.bin");
        owner.write("patches/golden/0003/shelf/record_book_full.bin", b"corrupt");
        owner.write("patches/golden/0003/shelf/obsolete.bin", b"stale");
        let before = owner.snapshot();
        let result = patch::goldens(&owner.0, 3, skein_codec_patches::patches::written);
        assert!(matches!(&result, Err(Error::Golden(_))), "{result:?}");
        let Err(Error::Golden(failures)) = result else { return };
        for name in ["record_book_smallest", "record_book_full", "obsolete"] {
            assert!(failures.iter().any(|s| s.contains(name)), "missing {name}: {failures:?}");
        }
        assert_eq!(owner.snapshot(), before);
    }

    #[test]
    fn s12_3_goldens_written_stops_report_no_site() {
        let owner = fixture();
        let result = patch::goldens(&owner.0, 3, no_site);
        assert!(matches!(&result, Err(Error::Golden(_))), "{result:?}");
        let Err(Error::Golden(failures)) = result else { return };
        assert_eq!(failures.len(), 8, "{failures:?}");
        assert!(failures.iter().all(|s| s.contains("patch 3 site 0") && s.contains("written Format")), "{failures:?}");
    }

    #[test]
    fn s12_3_declared_refusal_golden_matches_reason_and_rejects_wrong_reason() {
        fn refuse(_site: Site, _old: &Old, new: &mut Build, _at: Node, _cx: &mut ()) -> Result<(), Halt> {
            Err(new.refuse(RefusalIx::new(0)))
        }
        let (owner, landed) = baseline();
        owner.schema("alpha", 8, "record R { x: bool }");
        owner.run(&landed, &options());
        owner.write(
            "patches/0001.patch",
            format!("{}refuses bad \"not representable\"\n", owner.text("patches/0001.patch")),
        );
        for name in ["record_r_smallest", "record_r_full"] {
            owner.write(&format!("patches/golden/0001/alpha/{name}.bin.refused"), "refuses bad\n");
        }
        assert_eq!(patch::goldens(&owner.0, 1, refuse), Ok(()));
        owner.write("patches/golden/0001/alpha/record_r_full.bin.refused", "refuses different\n");
        let result = patch::goldens(&owner.0, 1, refuse);
        assert!(
            matches!(&result, Err(Error::Golden(names)) if names.iter().any(|s| s.contains("record_r_full"))),
            "{result:?}"
        );
    }

    #[test]
    fn s12_8_landed_read_roundtrips_only_kept_and_patch_paths() {
        let (owner, _) = frozen();
        owner.write("unrelated.txt", "not landed");
        let landed = owner.landed();
        assert!(landed.get(Path::new("schema/alpha-v8.schema")).is_none());
        assert!(landed.get(Path::new("unrelated.txt")).is_none());
        let files: BTreeMap<_, _> =
            landed.paths().map(|path| (path.to_owned(), landed.get(path).expect("listed bytes").to_vec())).collect();
        assert_eq!(Landed::new(files), landed);
        assert_eq!(Landed::none().paths().count(), 0);
    }

    #[test]
    fn s12_7_and_s11_2_added_values_checked_at_width_and_byte_bound() {
        for (ty, accepted, rejected) in [
            ("u8", "255", "256"),
            ("u16", "65535", "65536"),
            ("bool", "true", "truth"),
            ("text 2", "\"é\"", "\"éé\""),
            ("fixed 2", "0xaabb", "0xaa"),
            ("bytes 2", "0xaabb", "0xaabbcc"),
            ("list 2 u8", "[]", "false"),
            ("option u8", "some 255", "some 256"),
        ] {
            let (owner, landed) = baseline();
            owner.schema("alpha", 8, &format!("record R {{ x: u8, value: {ty} }}"));
            owner.run(&landed, &options());
            for (value, valid) in [(accepted, true), (rejected, false)] {
                owner.write(
                    "patches/0001.patch",
                    format!("patch 1\nowner qa 1\n\nfamily alpha 7 to 8\nadd R.value {ty} {value}\n"),
                );
                let before = owner.snapshot();
                let findings = patch::check(&owner.0, &landed).expect("check value");
                if valid {
                    assert!(findings.is_empty(), "{ty} {value}: {findings:?}");
                } else {
                    assert!(
                        findings
                            .iter()
                            .any(|f| matches!(f, Finding::Unplanned { patch: 1, family, .. } if family == "alpha")),
                        "{ty} {value}: {findings:?}"
                    );
                }
                assert_eq!(owner.snapshot(), before);
            }
        }
    }

    #[test]
    fn s12_3_compiled_owner_goldens_pass_without_writing() {
        let owner = fixture();
        let before = owner.snapshot();
        for number in [1, 2, 3] {
            assert_eq!(patch::goldens(&owner.0, number, skein_codec_patches::patches::written), Ok(()));
        }
        assert_eq!(owner.snapshot(), before);
    }

    #[test]
    fn s12_6_renumber_when_another_family_takes_the_number() {
        let (branch, base) = baseline();
        branch.schema("beta", 1, "record B { x: u8 }");
        branch.run(&base, &Options::default());
        let base = branch.landed();
        let upstream = branch.copy();
        branch.schema("alpha", 8, "record R { x: u16 }");
        branch.run(&base, &options());
        let alpha_goldens: BTreeMap<_, _> =
            branch.snapshot().into_iter().filter(|(path, _)| path.starts_with("patches/golden/0001/alpha")).collect();
        upstream.schema("beta", 2, "record B { x: u16 }");
        upstream.run(&base, &options());
        branch.schema("beta", 2, "record B { x: u16 }");
        let report = branch.run(&upstream.landed(), &Options::default());
        assert_eq!(report.moved, Some(1));
        assert_eq!(report.patch, Some(2));
        assert!(branch.text("patches/0002.patch").contains("family alpha 7 to 8"));
        assert!(!branch.text("patches/0002.patch").contains("family beta"));
        assert_eq!(branch.text("patches/0001.patch"), upstream.text("patches/0001.patch"));
        for (path, bytes) in alpha_goldens {
            let name = path.file_name().expect("golden name");
            assert_eq!(fs::read(branch.0.join("patches/golden/0002/alpha").join(name)).expect("moved golden"), bytes);
        }
        assert!(report.findings.is_empty(), "{report:?}");
    }
}
