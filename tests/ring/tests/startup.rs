//! Startup reads against real filesystem entries and bounded PEM bundles.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;

use skein_io::file::Unsafe;
use skein_io::kernel::{Error, Kind};
use skein_scratch::Scratch;
use skein_shell::{
    DirectoryAccess, RootBounds, Roots, RootsRefusal, Trust, Unread, directory_access, read_file, read_private_file,
    read_trust_roots,
};

#[cfg(target_os = "linux")]
const DESCRIPTORS: &str = "/proc/self/fd";
#[cfg(target_os = "macos")]
const DESCRIPTORS: &str = "/dev/fd";

const BOUNDS: RootBounds = RootBounds { bundle_bytes: 4_096, certificates: 8, certificate_bytes: 32 };

#[test]
fn directory_access_answers_writable_for_a_directory() {
    let scratch = Scratch::new("access");
    assert_eq!(directory_access(scratch.path()), DirectoryAccess::Writable);
}

#[test]
fn directory_access_answers_absent_for_a_missing_path() {
    let scratch = Scratch::new("access");
    assert_eq!(directory_access(&scratch.path().join("missing")), DirectoryAccess::Absent);
}

#[test]
fn directory_access_answers_not_a_directory_for_a_file() {
    let scratch = Scratch::new("access");
    let path = private_token(&scratch);
    assert_eq!(directory_access(&path), DirectoryAccess::NotDirectory);
}

#[test]
fn directory_access_answers_not_a_directory_for_a_file_in_the_path() {
    let scratch = Scratch::new("access");
    let path = private_token(&scratch);
    assert_eq!(directory_access(&path.join("child")), DirectoryAccess::NotDirectory);
}

#[cfg(target_os = "linux")]
fn mode_checks_bypassed() -> bool {
    let status = fs::read_to_string("/proc/self/status").expect("read effective capabilities");
    let capabilities = status.lines().find_map(|line| line.strip_prefix("CapEff:")).expect("CapEff").trim();
    u64::from_str_radix(capabilities, 16).expect("hex capabilities") & (1_u64 << 1_u32) != 0
}

#[cfg(target_os = "macos")]
fn mode_checks_bypassed() -> bool {
    let scratch = Scratch::new("mode-owner");
    let path = private_token(&scratch);
    fs::metadata(path).expect("new file's effective owner").uid() == 0
}

#[test]
fn directory_access_answers_denied_without_write_permission() {
    if mode_checks_bypassed() {
        eprintln!("effective privileges bypass directory modes; skipping the denied-write case");
        return;
    }
    let scratch = Scratch::new("access");
    fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o500)).expect("deny directory writes");
    assert_eq!(directory_access(scratch.path()), DirectoryAccess::Denied(libc::EACCES));
}

#[test]
fn directory_access_answers_denied_without_search_permission() {
    if mode_checks_bypassed() {
        eprintln!("effective privileges bypass directory modes; skipping the denied-search case");
        return;
    }
    let scratch = Scratch::new("access");
    fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o600)).expect("deny directory search");
    assert_eq!(directory_access(scratch.path()), DirectoryAccess::Denied(libc::EACCES));
}

#[test]
fn directory_access_answers_denied_for_a_nul_in_the_path() {
    assert_eq!(directory_access(Path::new("nul\0inside")), DirectoryAccess::Denied(libc::EINVAL));
}

fn entry_metadata(path: &Path) -> (u32, u64, i64, i64, i64, i64) {
    let entry = fs::metadata(path).expect("entry exists");
    (entry.mode(), entry.len(), entry.mtime(), entry.mtime_nsec(), entry.ctime(), entry.ctime_nsec())
}

#[test]
fn directory_access_creates_nothing_and_changes_no_entry() {
    let scratch = Scratch::new("access");
    let file = private_token(&scratch);
    let denied = scratch.path().join("denied");
    fs::create_dir(&denied).expect("plant a directory");
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o500)).expect("deny directory writes");
    let paths = [scratch.path().to_path_buf(), file.clone(), denied];
    let before = paths.each_ref().map(|path| entry_metadata(path));
    let descriptors = fs::read_dir(DESCRIPTORS).expect("descriptor observer").count();
    for path in &paths {
        let _answer = directory_access(path);
    }
    let absent = scratch.path().join("absent");
    let _answer = directory_access(&absent);
    assert!(!absent.exists(), "no missing entry created");
    assert_eq!(fs::read_dir(scratch.path()).expect("scratch entries").count(), 2, "no check file created");
    assert_eq!(paths.each_ref().map(|path| entry_metadata(path)), before, "modes, size and timestamps stay");
    assert_eq!(fs::read(&file).expect("file contents"), b"secret", "file contents stay");
    assert_eq!(fs::read_dir(DESCRIPTORS).expect("descriptor observer").count(), descriptors, "no descriptor kept");
}

fn private_token(scratch: &Scratch) -> std::path::PathBuf {
    let path = scratch.path().join("token");
    fs::write(&path, b"secret").expect("plant the token file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("make it private");
    path
}

#[test]
fn a_private_file_is_read_within_its_bound() {
    let scratch = Scratch::new("private");
    let path = private_token(&scratch);
    let user = skein_shell::effective_user();
    assert_eq!(read_private_file(&path, 6, user).expect("at bound").expose(), b"secret");
    assert_eq!(read_private_file(&path, 5, user), Err(Unread::TooLarge { max: 5 }));
}

#[test]
fn a_file_with_group_or_other_bits_is_refused_unread_as_mode() {
    let scratch = Scratch::new("private");
    let path = private_token(&scratch);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("make it group-readable");
    let user = skein_shell::effective_user();
    assert_eq!(read_private_file(&path, 64, user), Err(Unread::Unsafe(Unsafe::Mode(0o040))));
}

#[test]
fn a_symbolic_link_at_the_file_is_refused_as_link() {
    let scratch = Scratch::new("private");
    let token = private_token(&scratch);
    let path = scratch.path().join("link");
    symlink(token, &path).expect("plant a symbolic link");
    assert_eq!(read_private_file(&path, 64, skein_shell::effective_user()), Err(Unread::Unsafe(Unsafe::Link)));
}

#[test]
fn a_symbolic_link_in_a_parent_is_refused_as_link() {
    let scratch = Scratch::new("private");
    private_token(&scratch);
    let parent = scratch.path().join("parent-link");
    symlink(scratch.path(), &parent).expect("plant a parent symbolic link");
    assert_eq!(
        read_private_file(&parent.join("token"), 64, skein_shell::effective_user()),
        Err(Unread::Unsafe(Unsafe::Link))
    );
}

#[test]
fn a_file_with_two_links_is_refused_as_links() {
    let scratch = Scratch::new("private");
    let path = private_token(&scratch);
    fs::hard_link(&path, scratch.path().join("second-link")).expect("plant a second link");
    assert_eq!(read_private_file(&path, 64, skein_shell::effective_user()), Err(Unread::Unsafe(Unsafe::Links(2))));
}

#[test]
fn a_directory_and_a_fifo_are_refused_as_their_kind() {
    let scratch = Scratch::new("private");
    let fifo = scratch.path().join("fifo");
    assert!(Command::new("mkfifo").arg(&fifo).status().expect("mkfifo runs").success(), "plant a FIFO");
    let user = skein_shell::effective_user();
    assert_eq!(read_private_file(scratch.path(), 64, user), Err(Unread::Unsafe(Unsafe::Kind(Kind::Directory))));
    assert_eq!(read_private_file(&fifo, 64, user), Err(Unread::Unsafe(Unsafe::Kind(Kind::Other))));
}

#[test]
fn a_file_owned_by_another_expected_user_is_refused_as_owner() {
    let scratch = Scratch::new("private");
    let path = private_token(&scratch);
    let found = skein_shell::effective_user();
    let expected = found.checked_add(1).expect("the effective user is not the sentinel");
    assert_eq!(read_private_file(&path, 64, expected), Err(Unread::Unsafe(Unsafe::Owner { found, expected })));
}

#[test]
fn a_denied_own_file_is_named_by_its_path_only_stat() {
    if mode_checks_bypassed() {
        eprintln!("effective privileges bypass file modes; skipping the denied-open case");
        return;
    }
    let scratch = Scratch::new("private");
    let path = private_token(&scratch);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("deny the own file's read");
    assert_eq!(read_private_file(&path, 64, skein_shell::effective_user()), Err(Unread::Kernel(Error::Permission)));
}

#[test]
fn private_reads_leave_no_descriptor_open() {
    let scratch = Scratch::new("private");
    let path = private_token(&scratch);
    let user = skein_shell::effective_user();
    let descriptors = fs::read_dir(DESCRIPTORS).expect("descriptor observer").count();
    for _ in 0..8 {
        assert!(read_private_file(&path, 6, user).is_ok(), "safe file read");
        assert_eq!(read_private_file(&path, 5, user), Err(Unread::TooLarge { max: 5 }));
        assert_eq!(
            read_private_file(&path, 6, user + 1),
            Err(Unread::Unsafe(Unsafe::Owner { found: user, expected: user + 1 }))
        );
        assert_eq!(read_private_file(scratch.path(), 64, user), Err(Unread::Unsafe(Unsafe::Kind(Kind::Directory))));
        assert_eq!(read_private_file(&scratch.path().join("missing"), 64, user), Err(Unread::Absent));
        assert_eq!(read_private_file(Path::new("nul\0inside"), 64, user), Err(Unread::Kernel(Error::InvalidArgument)));
    }
    assert_eq!(fs::read_dir(DESCRIPTORS).expect("descriptor observer").count(), descriptors, "private reads close");
}

fn bundle(scratch: &Scratch, bytes: &[u8], bounds: RootBounds) -> Result<Roots, RootsRefusal> {
    let path = scratch.path().join("bundle");
    fs::write(&path, bytes).expect("plant the supplied bundle");
    read_trust_roots(&Trust::Machine { ssl_cert_file: Some(path) }, &bounds)
}

#[test]
fn startup_reads_whole_regular_files_at_their_bound_and_closes_every_entry() {
    let scratch = Scratch::new("startup");
    let path = scratch.path().join("file");
    let text = vec![0x31; 16_384];
    fs::write(&path, &text).expect("plant regular file");
    let fifo = scratch.path().join("fifo");
    assert!(Command::new("mkfifo").arg(&fifo).status().expect("mkfifo runs").success(), "plant a FIFO");
    let descriptors = fs::read_dir(DESCRIPTORS).expect("descriptor observer").count();
    for _ in 0..8 {
        assert_eq!(read_file(&path, 16_384).expect("exact bound").as_ref(), text);
        assert_eq!(read_file(&path, 16_383), Err(Unread::TooLarge { max: 16_383 }));
        assert_eq!(read_file(&fifo, 32), Err(Unread::NotAFile));
        assert_eq!(read_file(scratch.path(), 32), Err(Unread::NotAFile));
        assert_eq!(read_file(&scratch.path().join("missing"), 32), Err(Unread::Absent));
        assert_eq!(read_file(Path::new("nul\0inside"), 32), Err(Unread::Kernel(Error::InvalidArgument)));
    }
    assert_eq!(fs::read_dir(DESCRIPTORS).expect("descriptor observer").count(), descriptors, "all reads close");
    fs::write(&path, b"").expect("empty file");
    assert_eq!(read_file(&path, 0).expect("empty fits zero bytes").as_ref(), b"");
}

#[test]
fn pem_takes_der_in_order_and_skips_other_blocks_without_decoding_them() {
    let scratch = Scratch::new("startup");
    let bytes = b"explanatory text\r\n-----BEGIN PRIVATE KEY-----\ninvalid!\n-----END PRIVATE KEY-----\n\
        -----BEGIN CERTIFICATE-----\n T W\tF\x0bu\x0c \n-----END CERTIFICATE-----\n\
        -----BEGIN TRUSTED CERTIFICATE-----\nbroken!\n-----END TRUSTED CERTIFICATE-----\n\
        -----BEGIN CERTIFICATE-----\nAP8=\n-----END CERTIFICATE-----\n";
    let roots = bundle(&scratch, bytes, BOUNDS).expect("well-framed certificates");
    assert_eq!(roots.certificates.as_ref(), [Box::from(b"Man".as_slice()), Box::from([0, 255].as_slice())]);
    assert_eq!((roots.other_blocks, roots.malformed), (2, 0));
    assert_eq!(roots.path, scratch.path().join("bundle"));
}

#[test]
fn malformed_certificate_blocks_recover_at_the_next_begin_and_do_not_consume_the_count_bound() {
    let scratch = Scratch::new("startup");
    let valid = b"-----BEGIN CERTIFICATE-----\nTWFu\n-----END CERTIFICATE-----\n";
    for bad in [
        b"-----BEGIN CERTIFICATE-----\nTW!u\n-----END CERTIFICATE-----\n".as_slice(),
        b"-----BEGIN CERTIFICATE-----\nTWFu\n".as_slice(),
        b"-----BEGIN CERTIFICATE-----\nTWFu\n-----END KEY-----\n".as_slice(),
        b"-----BEGIN CERTIFICATE-----\nProc-Type: 4,ENCRYPTED\nTWFu\n-----END CERTIFICATE-----\n".as_slice(),
        b"-----BEGIN CERTIFICATE-----\nTW=Fu\n-----END CERTIFICATE-----\n".as_slice(),
        b"-----BEGIN CERTIFICATE-----\nT\n-----END CERTIFICATE-----\n".as_slice(),
    ] {
        let mut bytes = Vec::from(bad);
        bytes.extend_from_slice(valid);
        let roots = bundle(&scratch, &bytes, RootBounds { certificates: 1, ..BOUNDS }).expect("recover next root");
        assert_eq!(roots.certificates.as_ref(), [Box::from(b"Man".as_slice())]);
        assert_eq!(roots.malformed, 1, "one bad certificate block: {bad:?}");
    }
    let roots = bundle(&scratch, b"-----BEGIN CERTIFICATE-----\nTWFu", BOUNDS).expect("truncated block is skipped");
    assert!(roots.certificates.is_empty(), "none recovered");
    assert_eq!(roots.malformed, 1);
}

#[test]
fn root_bounds_refuse_and_first_decode_defect_or_byte_bound_wins() {
    let scratch = Scratch::new("startup");
    let valid = b"-----BEGIN CERTIFICATE-----\nTWFu\n-----END CERTIFICATE-----\n";
    assert_eq!(
        bundle(&scratch, valid, RootBounds { certificate_bytes: 2, ..BOUNDS }).expect_err("one byte past"),
        RootsRefusal::TooLong { index: 0, bound: 2 }
    );
    assert_eq!(
        bundle(&scratch, valid, RootBounds { certificates: 0, ..BOUNDS }).expect_err("no certificate allowed"),
        RootsRefusal::TooMany { bound: 0 }
    );
    let length = u32::try_from(valid.len()).expect("fixture fits");
    assert!(bundle(&scratch, valid, RootBounds { bundle_bytes: length, ..BOUNDS }).is_ok(), "exact bundle bytes");
    assert_eq!(
        bundle(&scratch, valid, RootBounds { bundle_bytes: length - 1, ..BOUNDS }).expect_err("bundle past bound"),
        RootsRefusal::Unread { path: scratch.path().join("bundle"), why: Unread::TooLarge { max: length - 1 } }
    );
    let bad_first = b"-----BEGIN CERTIFICATE-----\n!TWFu\n-----END CERTIFICATE-----\n";
    let roots = bundle(&scratch, bad_first, RootBounds { certificate_bytes: 0, ..BOUNDS }).expect("first defect wins");
    assert_eq!(roots.malformed, 1);
    let bound_first = b"-----BEGIN CERTIFICATE-----\nTWFu!\n-----END CERTIFICATE-----\n";
    assert_eq!(
        bundle(&scratch, bound_first, RootBounds { certificate_bytes: 0, ..BOUNDS }).expect_err("first bound wins"),
        RootsRefusal::TooLong { index: 0, bound: 0 }
    );
    let mut after_bad = Vec::from(bad_first);
    after_bad.extend_from_slice(valid);
    assert_eq!(
        bundle(&scratch, &after_bad, RootBounds { certificate_bytes: 2, ..BOUNDS })
            .expect_err("second certificate long"),
        RootsRefusal::TooLong { index: 1, bound: 2 }
    );
}

#[test]
fn startup_reads_one_der_certificate_and_honors_the_explicit_bundle_path() {
    let scratch = Scratch::new("startup");
    let path = scratch.path().join("root.der");
    fs::write(&path, b"DER bytes").expect("plant DER");
    let roots = read_trust_roots(&Trust::Der(path.clone()), &BOUNDS).expect("DER file");
    assert_eq!(roots.certificates.as_ref(), [Box::from(b"DER bytes".as_slice())]);
    assert_eq!((roots.other_blocks, roots.malformed), (0, 0));
    assert_eq!(roots.path, path);
    assert_eq!(
        read_trust_roots(&Trust::Der(path), &RootBounds { certificate_bytes: 8, ..BOUNDS })
            .expect_err("DER byte bound"),
        RootsRefusal::TooLong { index: 0, bound: 8 }
    );
    let missing = scratch.path().join("missing");
    assert_eq!(
        read_trust_roots(&Trust::Machine { ssl_cert_file: Some(missing.clone()) }, &BOUNDS)
            .expect_err("explicit absent"),
        RootsRefusal::Unread { path: missing, why: Unread::Absent }
    );
}
