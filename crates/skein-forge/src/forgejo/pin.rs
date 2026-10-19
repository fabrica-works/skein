/// The pinned Forgejo release: the one whose behaviour the conformance checked, and owned mode runs.
pub const VERSION: &str = "17.0.0";
/// The pinned forgejo-runner release.
pub const RUNNER_VERSION: &str = "13.2.0";
/// Whether Forgejo's first `--version` line names `VERSION`, ended by nothing, `+` or a space.
#[must_use]
pub fn names_forgejo(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("forgejo version ") else {
        return false;
    };
    rest.split(['+', ' ']).next() == Some(VERSION)
}
/// Whether forgejo-runner's first `--version` line is `forgejo-runner version v` and `RUNNER_VERSION`, nothing more.
#[must_use]
pub fn names_runner(line: &str) -> bool {
    line.strip_prefix("forgejo-runner version v") == Some(RUNNER_VERSION)
}
