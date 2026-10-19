use crate::forgejo::pin::{RUNNER_VERSION, VERSION, names_forgejo, names_runner};

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
