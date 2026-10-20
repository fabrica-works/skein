/// The pinned Forgejo release: the one whose behaviour the conformance checked, and owned mode runs.
pub const VERSION: &str = "17.0.0";
/// The pinned forgejo-runner release.
pub const RUNNER_VERSION: &str = "13.2.0";

/// SHA-256 of `forgejo-17.0.0-linux-amd64`.
pub const FORGEJO_SHA256_LINUX_AMD64: &str = "5a3b07e124ed70b4e46ee3939515f003bd3cc85352257a9a4607a527251042ca";
/// SHA-256 of `forgejo-17.0.0-linux-arm64`.
pub const FORGEJO_SHA256_LINUX_ARM64: &str = "e5ec437cd6357e8d1b4580cb47ab1daa58a32b33971a63db63446a535d33adb7";
/// SHA-256 of `forgejo-runner-13.2.0-linux-amd64`.
pub const RUNNER_SHA256_LINUX_AMD64: &str = "fadaec897f5e6641c363f87ecaf75f866f99c81b7097ec6b433e4c48123fcad1";
/// SHA-256 of `forgejo-runner-13.2.0-linux-arm64`.
pub const RUNNER_SHA256_LINUX_ARM64: &str = "9d3d462ee83e4e629f959faeacceb20eed2f25290bddea402e65ace46a31afd9";

/// A binary pinned to its release and digests (forge.md, 4.3).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Binary {
    Forgejo,
    Runner,
}

/// The pinned Linux digest for the release architecture name `amd64` or `arm64`.
/// Other names return `None`; the caller names the architecture when it refuses
/// (forge.md, 4.3, "An architecture without a digest").
#[must_use]
pub fn digest(binary: Binary, arch: &str) -> Option<&'static str> {
    let (amd64, arm64) = match binary {
        Binary::Forgejo => (FORGEJO_SHA256_LINUX_AMD64, FORGEJO_SHA256_LINUX_ARM64),
        Binary::Runner => (RUNNER_SHA256_LINUX_AMD64, RUNNER_SHA256_LINUX_ARM64),
    };
    match arch {
        "amd64" => Some(amd64),
        "arm64" => Some(arm64),
        _ => None,
    }
}

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
