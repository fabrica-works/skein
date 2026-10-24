//! Each row of codec.md 12.7, checked without writing.
extern crate alloc;

#[cfg(test)]
mod tests {

    use skein_codegen::diff::Version;
    use skein_codegen::patch::{Error, Finding, Landed, check};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const V1: &str = "family shelf 1\nrecord Book { id: u8 }\nenum State { ready }\n";
    const V2: &str = "family shelf 2\nrecord Book { id: u8, score: u32 }\nenum State { ready }\n";
    const PATCH: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nadd Book.score u32 7\n";

    fn crate_files(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("skein-patch-check-{}-{name}", std::process::id()));
        fs::create_dir(&root).expect("unique scratch crate");
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().expect("file parent")).expect("directories");
            fs::write(path, text).expect("fixture file");
        }
        root
    }

    fn assert_files(root: &Path, files: &[(&str, &str)]) {
        for (path, text) in files {
            assert_eq!(fs::read_to_string(root.join(path)).expect(path), *text, "{path}");
        }
    }

    fn findings(name: &str, files: &[(&str, &str)], landed: &Landed, expected: &[Finding]) {
        let root = crate_files(name, files);
        assert_eq!(check(&root, landed), Ok(expected.to_vec()));
        assert_files(&root, files);
        fs::remove_dir_all(root).expect("remove scratch crate");
    }

    fn base(files: &[(&str, &str)]) -> Landed {
        Landed::new(files.iter().map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec())).collect())
    }

    #[test]
    fn current_crate_has_no_finding_and_is_never_written() {
        let files = [
            ("schema/shelf-v2.schema", V2),
            ("schema/kept/shelf-v1.schema", V1),
            ("schema/kept/shelf-v2.schema", V2),
            ("patches/0001.patch", PATCH),
        ];
        let root = crate_files("clean", &files);
        assert_eq!(check(&root, &base(&files)), Ok(Vec::new()));
        assert_files(&root, &files);
        fs::remove_dir_all(root).expect("remove scratch crate");
    }

    #[test]
    fn current_without_kept_text_is_unkept() {
        findings(
            "unkept",
            &[("schema/shelf-v1.schema", V1)],
            &Landed::none(),
            &[Finding::Unkept { family: "shelf".into() }],
        );
    }

    #[test]
    fn missing_middle_kept_version_is_a_gap() {
        const V3: &str = "family shelf 3\nrecord Book { id: u8 }\nenum State { ready }\n";
        findings(
            "gap",
            &[("schema/kept/shelf-v1.schema", V1), ("schema/kept/shelf-v3.schema", V3)],
            &Landed::none(),
            &[
                Finding::KeptGap { family: "shelf".into(), missing: 2 },
                Finding::Unpatched { family: "shelf".into(), version: 3 },
            ],
        );
    }

    #[test]
    fn later_kept_version_requires_a_patch_to_it() {
        findings(
            "unpatched-kept",
            &[("schema/kept/shelf-v1.schema", V1), ("schema/kept/shelf-v2.schema", V2)],
            &Landed::none(),
            &[Finding::Unpatched { family: "shelf".into(), version: 2 }],
        );
    }

    #[test]
    fn current_needing_patch_requires_one_to_its_version() {
        findings(
            "unpatched-current",
            &[("schema/shelf-v2.schema", V2), ("schema/kept/shelf-v1.schema", V1)],
            &Landed::none(),
            &[Finding::Unpatched { family: "shelf".into(), version: 2 }],
        );
    }

    #[test]
    fn unfrozen_comments_are_lagging_until_the_run_refreshes() {
        const COMMENTED: &str = "# current comment\nfamily shelf 1\nrecord Book { id: u8 }\nenum State { ready }\n";
        findings(
            "lagging",
            &[("schema/shelf-v1.schema", COMMENTED), ("schema/kept/shelf-v1.schema", V1)],
            &Landed::none(),
            &[Finding::Lagging { family: "shelf".into(), version: 1 }],
        );
    }

    #[test]
    fn wrong_current_version_names_both_versions() {
        const V7: &str = "family shelf 7\nrecord Book { id: u8 }\nenum State { ready }\n";
        const V9: &str = "family shelf 9\nrecord Book { id: u8 }\nenum State { ready }\n";
        findings(
            "wrong",
            &[("schema/shelf-v9.schema", V9), ("schema/kept/shelf-v7.schema", V7)],
            &Landed::none(),
            &[Finding::Version { family: "shelf".into(), verdict: Version::Wrong, kept: 7, current: 9 }],
        );
    }

    #[test]
    fn moved_version_without_layout_change_is_needless() {
        const SAME2: &str = "family shelf 2\nrecord Book { id: u8 }\nenum State { ready }\n";
        findings(
            "needless",
            &[("schema/shelf-v2.schema", SAME2), ("schema/kept/shelf-v1.schema", V1)],
            &Landed::none(),
            &[Finding::Version { family: "shelf".into(), verdict: Version::Needless, kept: 1, current: 2 }],
        );
    }

    #[test]
    fn needed_layout_change_at_kept_version_is_unmoved() {
        const WIDE1: &str = "family shelf 1\nrecord Book { id: u16 }\nenum State { ready }\n";
        findings(
            "unmoved",
            &[("schema/shelf-v1.schema", WIDE1), ("schema/kept/shelf-v1.schema", V1)],
            &Landed::none(),
            &[
                Finding::Unpatched { family: "shelf".into(), version: 1 },
                Finding::Version { family: "shelf".into(), verdict: Version::Unmoved, kept: 1, current: 1 },
            ],
        );
    }

    #[test]
    fn changed_or_missing_landed_files_are_reported_by_path() {
        findings(
            "changed",
            &[("patches/golden/0001/shelf/book", "edited")],
            &base(&[
                ("schema/kept/shelf-v1.schema", V1),
                ("patches/0001.patch", PATCH),
                ("patches/golden/0001/shelf/book", "landed"),
            ]),
            &[
                Finding::Changed { path: "patches/0001.patch".into() },
                Finding::Changed { path: "patches/golden/0001/shelf/book".into() },
                Finding::Changed { path: "schema/kept/shelf-v1.schema".into() },
            ],
        );
    }

    #[test]
    fn refreshed_unfrozen_kept_text_is_exempt_from_changed() {
        const RENAMED: &str = "family shelf 1\nrecord Book { key: u8 }\nenum State { ready }\n";
        let files = [("schema/shelf-v1.schema", RENAMED), ("schema/kept/shelf-v1.schema", RENAMED)];
        let root = crate_files("refresh-exempt", &files);
        assert_eq!(check(&root, &base(&[("schema/kept/shelf-v1.schema", V1)])), Ok(Vec::new()));
        assert_files(&root, &files);
        fs::remove_dir_all(root).expect("remove scratch crate");
    }

    #[test]
    fn missing_patch_number_is_reported_without_writing_it() {
        findings(
            "patch-gap",
            &[("patches/0002.patch", "patch 1\nowner shop 2\n")],
            &Landed::none(),
            &[Finding::PatchGap { missing: 1 }],
        );
    }

    #[test]
    fn patch_level_must_match_its_filename_number() {
        findings(
            "misnumbered",
            &[("patches/0001.patch", "patch 1\nowner shop 7\n")],
            &Landed::none(),
            &[Finding::Misnumbered { patch: 1, level: 7 }],
        );
    }

    #[test]
    fn asked_value_is_unplanned_with_line_span_and_variant_name() {
        const ASKED: &str = "patch 1\nowner shop 1\n\nfamily shelf 1 to 2\nadd Book.score u32 ?\n";
        let root = crate_files(
            "asked",
            &[("schema/kept/shelf-v1.schema", V1), ("schema/kept/shelf-v2.schema", V2), ("patches/0001.patch", ASKED)],
        );
        let result = check(&root, &Landed::none()).expect("the crate reads");
        assert_eq!(
            result,
            [Finding::Unplanned { patch: 1, family: "shelf".into(), error: "line 5 at 61..62: Asked".into() }]
        );
        fs::remove_dir_all(root).expect("remove scratch crate");
    }

    #[test]
    fn missing_kept_text_prevents_planning_its_block() {
        findings(
            "unplanned-kept",
            &[("schema/kept/shelf-v1.schema", V1), ("patches/0001.patch", PATCH)],
            &Landed::none(),
            &[Finding::Unplanned { patch: 1, family: "shelf".into(), error: "line 4 at 22..28: Unkept".into() }],
        );
    }

    #[test]
    fn a_crate_the_check_cannot_read_is_its_error_not_a_finding() {
        let root = crate_files("read-error", &[("schema/shelf-v1.schema", "invalid")]);
        let error = check(&root, &Landed::none()).expect_err("schema read failed");
        let Error::Schema { path, .. } = error else { unreachable!("a schema that does not parse") };
        assert_eq!(path, Path::new("schema/shelf-v1.schema"));
        fs::remove_dir_all(root).expect("remove scratch crate");
    }

    #[test]
    fn each_finding_displays_the_required_fix_or_refusal() {
        for (finding, expected) in [
            (Finding::Unkept { family: "shelf".into() }, "shelf: no kept text; run `skein-codegen patch <crate>`"),
            (
                Finding::KeptGap { family: "shelf".into(), missing: 2 },
                "shelf: kept version 2 missing; kept versions run without a gap",
            ),
            (
                Finding::Unpatched { family: "shelf".into(), version: 2 },
                "shelf: no patch moves it to version 2; run `skein-codegen patch <crate>`",
            ),
            (
                Finding::Lagging { family: "shelf".into(), version: 1 },
                "shelf: kept version 1 lags the current text; run `skein-codegen patch <crate>`",
            ),
            (
                Finding::Version { family: "shelf".into(), verdict: Version::Wrong, kept: 7, current: 9 },
                "shelf: version 9 over kept 7 is Wrong (codec.md, 12.5)",
            ),
            (Finding::Changed { path: "patches/0001.patch".into() }, "patches/0001.patch: landed, and changed or gone"),
            (Finding::PatchGap { missing: 1 }, "patch 0001 missing"),
            (Finding::Misnumbered { patch: 1, level: 7 }, "patch 0001: its header's level is 7"),
            (
                Finding::Unplanned { patch: 1, family: "shelf".into(), error: "line 5: Asked".into() },
                "patch 0001, shelf: does not plan: line 5: Asked",
            ),
        ] {
            assert_eq!(finding.to_string(), expected);
        }
    }

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("scratch git");
        assert!(output.status.success(), "git {}: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    fn binary_reads_crate_relative_base_and_returns_all_exit_codes() {
        let root = crate_files("binary", &[("nested/owner/schema/shelf-v1.schema", V1)]);
        git(&root, &["init", "--quiet"]);
        git(&root, &["add", "."]);
        git(
            &root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "baseline",
            ],
        );
        let owner = root.join("nested/owner");
        let output = Command::new(env!("CARGO_BIN_EXE_skein-codegen"))
            .arg("patch")
            .arg(&owner)
            .args(["--base", "HEAD", "--baseline"])
            .output()
            .expect("binary baseline");
        assert_eq!(output.status.code(), Some(0_i32));
        assert_eq!(String::from_utf8(output.stdout).expect("report utf8"), "shelf: kept version 1, its first\n");
        git(&root, &["add", "."]);
        git(
            &root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "kept",
            ],
        );
        fs::write(owner.join("schema/shelf-v2.schema"), V2).expect("current v2");
        let output = Command::new(env!("CARGO_BIN_EXE_skein-codegen"))
            .arg("patch")
            .arg(&owner)
            .args(["--base", "HEAD", "--owner", "shop"])
            .output()
            .expect("binary patch");
        assert_eq!(output.status.code(), Some(1_i32));
        assert!(String::from_utf8(output.stdout).expect("utf8").contains("Asked"), "planner refusal printed");
        let before = Landed::read(&owner).expect("tree snapshot");
        let output = Command::new(env!("CARGO_BIN_EXE_skein-codegen"))
            .arg("patch")
            .arg(&owner)
            .args(["--base", "HEAD", "--check"])
            .output()
            .expect("binary check");
        assert_eq!(output.status.code(), Some(1_i32));
        assert_eq!(Landed::read(&owner).expect("tree snapshot"), before);
        let output = Command::new(env!("CARGO_BIN_EXE_skein-codegen")).arg("patch").output().expect("binary error");
        assert_eq!(output.status.code(), Some(2_i32));
        fs::write(owner.join("schema/shelf-v2.schema"), "invalid").expect("invalid current");
        let output = Command::new(env!("CARGO_BIN_EXE_skein-codegen"))
            .arg("patch")
            .arg(&owner)
            .args(["--base", "HEAD", "--check"])
            .output()
            .expect("binary read error");
        assert_eq!(output.status.code(), Some(2_i32));
        assert!(
            String::from_utf8(output.stderr).expect("utf8").contains("schema/shelf-v2.schema"),
            "read error printed"
        );
        fs::remove_dir_all(root).expect("remove scratch git");
    }
}
