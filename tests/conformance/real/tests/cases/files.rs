use super::BACKEND;
use crate::on;
use skein_conformance::Check;
use skein_conformance::{
    Pairing, PathLimits, Shortness, cancel_read, escapes, file_lifecycle, file_metadata, list, make_directory,
    nested_roots, permissions, remove, rename,
};
use skein_conformance_real::Real;

/// A regular file's reads and writes count every byte they could: one of
/// the outcomes the contract allows, the one the calm simulator gives.
#[test]
fn a_file_is_made_written_at_offsets_read_back_and_stated() {
    let seen = file_lifecycle(&mut Real::new(BACKEND));
    seen.check();
    let whole = Shortness { short: false, full: true };
    assert_eq!((seen.reads, seen.writes()), (whole, whole), "{seen:?}");
}

#[test]
fn renames_over_across_and_beneath() {
    on(BACKEND, rename);
}

#[test]
fn removes_of_files_directories_and_links() {
    on(BACKEND, remove);
}

#[test]
fn new_directories_and_one_removed_while_open() {
    on(BACKEND, |backend| make_directory(backend, PathLimits::HOST));
}

#[test]
fn listings_whole_one_at_a_time_and_cut_short() {
    on(BACKEND, list);
}

#[test]
fn a_root_beneath_a_root() {
    on(BACKEND, nested_roots);
}

#[test]
fn paths_that_leave_their_root_and_paths_that_stay() {
    on(BACKEND, escapes);
}

#[test]
fn what_the_owner_may_not_do() {
    assert!(Real::permissions_checked(), "run as root, whom no mode stops: run as a user");
    on(BACKEND, |backend| permissions(backend, PathLimits::HOST));
}

/// A file's `Read` goes to the ring's worker, even on tmpfs, so its `Cancel`
/// stops it, or comes once it has read: either, as the simulator draws
/// them. That a `Cancel` stops one that waits on a filesystem that stalls,
/// no scratch directory shows; the simulator's `hung` fault does.
#[test]
fn a_cancel_of_a_read_of_a_file_stops_it_or_is_too_late() {
    let seen = cancel_read(&mut Real::new(BACKEND));
    seen.check();
    let drawn = [Pairing::Stopped, Pairing::Interrupted, Pairing::Completed];
    assert!(drawn.contains(&seen.pairing()), "a pairing the simulator draws: {seen:?}");
}

#[test]
fn appends_preserve_the_prefix_and_follow_the_other_descriptors_bytes() {
    on(BACKEND, skein_conformance::appending);
}

#[test]
fn a_cancel_of_an_append_stops_it_or_comes_too_late() {
    on(BACKEND, skein_conformance::cancel_append);
}

#[test]
fn stat_keeps_the_owner_and_counts_hard_links_through_removal() {
    use std::os::unix::fs::MetadataExt;

    let seen = file_metadata(&mut Real::new(BACKEND));
    seen.check();
    let scratch = skein_scratch::Scratch::new("stat-owner");
    let path = scratch.path().join("owner");
    std::fs::write(&path, b"").expect("create a file as the effective user");
    let owner = std::fs::metadata(path).expect("new file's effective owner").uid();
    assert_eq!(seen.root.owner, owner, "Stat retains the effective user ID");
}

#[test]
fn path_only_metadata_needs_no_permission_on_its_entry() {
    #[cfg(target_os = "linux")]
    let denial_expected = {
        let status = std::fs::read_to_string("/proc/self/status").expect("Linux capability status");
        let capabilities =
            status.lines().find_map(|line| line.strip_prefix("CapEff:")).expect("effective capabilities").trim();
        let capabilities = u64::from_str_radix(capabilities, 16).expect("hexadecimal effective capabilities");
        capabilities & (1_u64 << 1_u32) == 0
    };
    #[cfg(target_os = "macos")]
    let denial_expected = {
        use std::os::unix::fs::MetadataExt;

        let scratch = skein_scratch::Scratch::new("mode-owner");
        let path = scratch.path().join("owner");
        std::fs::write(&path, b"").expect("create a file as the effective user");
        std::fs::metadata(path).expect("new file's effective owner").uid() != 0
    };
    if !denial_expected {
        eprintln!(
            "effective privileges bypass mode 000: skipping only the Read-denial assertions; path-only Stat and Close are checked"
        );
    }
    skein_conformance::path_metadata(&mut Real::new(BACKEND), denial_expected, PathLimits::HOST).check();
}

#[test]
fn a_write_open_finds_what_exists_and_truncate_sets_its_length() {
    on(BACKEND, skein_conformance::cutting);
}

#[test]
fn a_directory_lock_is_busy_through_a_second_open_until_the_first_closes() {
    skein_conformance::lock(&mut Real::new(BACKEND), env!("CARGO_BIN_EXE_process_fixture").as_bytes()).check();
}

#[test]
fn a_no_replace_rename_refuses_a_taken_target_in_linux_order() {
    on(BACKEND, |backend| skein_conformance::no_replace(backend, PathLimits::HOST));
}
