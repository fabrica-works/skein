use crate::forgejo::pin::{
    Binary, FORGEJO_SHA256_LINUX_AMD64, FORGEJO_SHA256_LINUX_ARM64, RUNNER_SHA256_LINUX_AMD64,
    RUNNER_SHA256_LINUX_ARM64, RUNNER_VERSION, VERSION, digest, names_forgejo, names_runner,
};

#[test]
fn digest_answers_only_the_pinned_release_architectures() {
    for (binary, arch, expected) in [
        (Binary::Forgejo, "amd64", Some(FORGEJO_SHA256_LINUX_AMD64)),
        (Binary::Forgejo, "arm64", Some(FORGEJO_SHA256_LINUX_ARM64)),
        (Binary::Runner, "amd64", Some(RUNNER_SHA256_LINUX_AMD64)),
        (Binary::Runner, "arm64", Some(RUNNER_SHA256_LINUX_ARM64)),
        (Binary::Forgejo, "x86_64", None),
        (Binary::Runner, "x86_64", None),
        (Binary::Forgejo, "aarch64", None),
        (Binary::Runner, "aarch64", None),
        (Binary::Forgejo, "386", None),
        (Binary::Runner, "386", None),
        (Binary::Forgejo, "AMD64", None),
        (Binary::Runner, "AMD64", None),
        (Binary::Forgejo, "", None),
        (Binary::Runner, "", None),
    ] {
        assert_eq!(digest(binary, arch), expected, "{binary:?} {arch:?}");
    }
}

#[test]
fn pinned_digests_are_64_lowercase_hex_digits() {
    for pinned in
        [FORGEJO_SHA256_LINUX_AMD64, FORGEJO_SHA256_LINUX_ARM64, RUNNER_SHA256_LINUX_AMD64, RUNNER_SHA256_LINUX_ARM64]
    {
        assert_eq!(pinned.len(), 64, "{pinned}");
        for byte in pinned.bytes() {
            assert!(byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte), "{pinned}");
        }
    }
}

#[test]
fn forgejo_s_line_with_its_build_details_names_the_pin() {
    for line in [
        "forgejo version @+gitea-1.22.0 (release name @) built with GNU Make 4.4.1, go1.26.9 : bindata, timetzdata, sqlite, sqlite_unlock_notify".replace('@', VERSION),
        "forgejo version @".replace('@', VERSION),
    ] {
        assert!(names_forgejo(&line), "{line}");
    }
}
#[test]
fn a_forgejo_version_extending_the_pin_is_refused() {
    for line in [
        "forgejo version 1@+gitea-1.22.0".replace('@', VERSION),
        "forgejo version @0+gitea-1.22.0".replace('@', VERSION),
        "forgejo version @-rc1".replace('@', VERSION),
        "gitea version @+gitea-1.22.0".replace('@', VERSION),
    ] {
        assert!(!names_forgejo(&line), "{line}");
    }
}
#[test]
fn the_runner_s_version_line_names_the_runner_pin() {
    assert!(names_runner(&"forgejo-runner version v@".replace('@', RUNNER_VERSION)));
}
#[test]
fn a_runner_version_containing_the_pin_is_refused() {
    for line in [
        "forgejo-runner version v1@".replace('@', RUNNER_VERSION),
        "forgejo-runner version v@0".replace('@', RUNNER_VERSION),
        "forgejo-runner version v@-dev".replace('@', RUNNER_VERSION),
        "forgejo-runner version @".replace('@', RUNNER_VERSION),
    ] {
        assert!(!names_runner(&line), "{line}");
    }
}
