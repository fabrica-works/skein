// No owned Forgejo on macOS: the pinned binary releases are Linux's (shell.md, 9).
#![cfg(target_os = "linux")]

use skein_forge::forgejo::pin::VERSION;
use skein_throwaway_forgejo::{Failure, Forgejo, Init, Outgoing, Refusal, Settings};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};
#[test]
fn a_binary_printing_another_version_is_refused_naming_it() {
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("wrong-version");
    fs::create_dir_all(&scratch).expect("scratch");
    let binary = scratch.join("forgejo");
    fs::write(&binary, b"#!/bin/sh\nprintf 'forgejo version 15.0.0+gitea-1.22.0\\n'\n").expect("script");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).expect("executable");
    assert_eq!(
        Forgejo::from_path(&binary).expect_err("refused"),
        Refusal::Version {
            path: fs::canonicalize(&binary).expect("absolute"),
            printed: "forgejo version 15.0.0+gitea-1.22.0".into()
        }
    );
    fs::remove_dir_all(scratch).expect("removed");
}
#[test]
fn a_scratch_directory_that_is_not_empty_is_refused() {
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("occupied");
    fs::create_dir_all(&scratch).expect("scratch");
    fs::write(scratch.join("keep"), b"mine").expect("file");
    let binary = scratch.join("forgejo");
    fs::write(&binary, format!("#!/bin/sh\nprintf 'forgejo version {VERSION}+gitea-1.22.0 (release name {VERSION}) built with GNU Make 4.4.1, go1.26.9\\n'\n")).expect("script");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).expect("executable");
    let forgejo = Forgejo::from_path(&binary).expect("version accepted");
    let result = forgejo.start(&scratch, &Settings::small());
    assert!(matches!(result, Err(Failure::Scratch { .. })));
    assert_eq!(fs::read(scratch.join("keep")).expect("kept"), b"mine");
    fs::remove_dir_all(scratch).expect("removed");
}
#[test]
#[ignore = "needs SKEIN_FORGEJO"]
fn a_throwaway_forgejo_starts_serves_its_admin_and_is_removed() {
    let forgejo = Forgejo::from_path(Path::new(
        &std::env::var_os("SKEIN_FORGEJO").expect("SKEIN_FORGEJO must name the pinned Forgejo"),
    ))
    .expect("pinned Forgejo");
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("throwaway-start");
    let mut instance = forgejo.start(&scratch, &Settings::small()).expect("started");
    let admin = instance.admin().clone();
    let user = instance.exchange(&Outgoing::get("/api/v1/user", Some(&admin.token))).expect("answered");
    assert_eq!(user.status, 200, "the admin's token reads its user");
    let made = instance.create_repository(&admin, "specimen", Init::Readme).expect("made");
    assert_eq!(made.name, "specimen");
    instance.stop().expect("stopped");
    assert!(!scratch.exists(), "the scratch directory is removed");
}

/// A pinned Forgejo version followed by the supplied command body.
fn forgejo_running(root: &Path, body: &str) -> Forgejo {
    fs::create_dir_all(root).expect("root");
    let binary = root.join("forgejo");
    fs::write(
        &binary,
        format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'forgejo version {VERSION}'; exit 0; fi\n{body}\n"),
    )
    .expect("script");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).expect("executable");
    Forgejo::from_path(&binary).expect("version accepted")
}
#[test]
fn a_work_path_runs_commands_in_the_scratch_with_its_environment() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("work-path-run");
    let forgejo = forgejo_running(&root, "pwd; echo \"$HOME $PATH $TZ\"; echo \"$@\"");
    let work = forgejo.work_path(&root.join("work"), &Settings::small()).expect("prepared");
    let output = work.run(&["admin", "user", "list"]).expect("ran");
    let w = work.scratch().display();
    assert_eq!(
        output.stdout,
        format!("{w}\n{w}/home /usr/bin:/bin Asia/Kolkata\n--work-path {w} --config {w}/app.ini admin user list\n")
    );
    assert!(work.scratch().join("tmp").is_dir());
    fs::remove_dir_all(root).expect("removed");
}
#[test]
fn a_failed_work_path_command_keeps_its_exit_code_and_both_streams() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("work-path-fail");
    let forgejo = forgejo_running(&root, "echo output; echo diagnostic >&2; exit 7");
    let work = forgejo.work_path(&root.join("work"), &Settings::small()).expect("prepared");
    let output = work.run(&["migrate"]).expect("ran");
    assert_eq!(output.code, Some(7));
    assert_eq!(output.stdout, "output\n");
    assert_eq!(output.stderr, "diagnostic\n");
    fs::remove_dir_all(root).expect("removed");
}
#[test]
fn a_work_path_is_refused_when_its_scratch_is_not_empty() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("work-path-occupied");
    let forgejo = forgejo_running(&root, "exit 0");
    let work = root.join("work");
    fs::create_dir_all(&work).expect("work");
    fs::write(work.join("keep"), b"kept").expect("file");
    match forgejo.work_path(&work, &Settings::small()) {
        Err(Failure::Scratch { path, why }) => {
            assert_eq!(path, work);
            assert_eq!(why, "scratch is not empty");
        }
        other => panic!("expected scratch refusal, got success: {}", other.is_ok()),
    }
    assert_eq!(fs::read(work.join("keep")).expect("retained"), b"kept");
    fs::remove_dir_all(root).expect("removed");
}

mod support;
#[test]
fn attach_reads_the_admin_login_and_id() {
    let fixture = support::attached("kit-owner", 37);
    assert_eq!(fixture.instance.admin().login, "kit-owner");
    assert_eq!(fixture.instance.admin().id, 37);
    fixture.instance.stop().expect("stopped");
    fs::remove_dir_all(fixture.scratch).expect("owner removes scratch");
}
#[test]
fn stopping_an_attached_instance_reaps_the_child_and_keeps_scratch() {
    let fixture = support::attached("kit-owner", 37);
    fixture.instance.stop().expect("stopped");
    assert!(!Path::new(&format!("/proc/{}", fixture.pid)).exists());
    assert_eq!(fs::read(fixture.scratch.join("keep")).expect("owner file"), b"mine");
    fs::remove_dir_all(fixture.scratch).expect("owner removes scratch");
}
#[test]
fn an_attached_instance_refuses_cli() {
    let fixture = support::attached("kit-owner", 37);
    match fixture.instance.cli(&[]) {
        Err(Failure::Io(error)) => assert_eq!(error, "attached instance has no offline work path"),
        other => panic!("expected offline path refusal: {other:?}"),
    }
    fixture.instance.stop().expect("stopped");
    fs::remove_dir_all(fixture.scratch).expect("owner removes scratch");
}
