# Gate policy

Provisional, 2026-10-22. The runtime contract for reusable gate evidence.

## 1. In one page

- **One effective policy.** `scripts/dev/gate-policy.sh` provides
  `gate_policy_prepare`, which validates and exports `SKEIN_TEST_CHROMIUM`,
  `SKEIN_TEST_FONTS` and `SKEIN_GATE_POLICY_ID`. The prepared
  `SKEIN_GATE_POLICY_HELPER` path lets the repository gate use the same kit
  helper as admission while a change to that helper is being landed.
  `gate_policy_identity`
  prepares in a subshell and prints only the identity. Errors disclose no
  environment or configuration values.
- **The kit selects Chromium.** The default installation is
  `~/.local/opt/chromium-147.0.7727.101/`. The kit resolves its launcher to
  `usr/lib/chromium/chromium`, avoiding the launcher's sourced settings.
  An explicit `SKEIN_TEST_CHROMIUM` is accepted only when executable and
  reporting Chromium 147.0.7727.101. PATH cannot select the gate's browser.
- **Evidence binds policy.** A code landing prepares before cache lookup,
  passes the exports to the build and gate lanes, and prepares again before
  accepting the result. Its cache record and `Gate-Policy` trailer carry
  the identity. Evidence without a policy is invalid. Documentation-only
  differences under `docs/` preserve evidence only under the same policy.
- **The gate stays full.** `scripts/check.sh` validates at entry, then runs
  all of its checks and the focused, fuzzy and browser suites. Deliberate
  wrapper overrides for direct-rustc work and benchmarks remain possible.

## 2. Identity

The identity is a versioned SHA-256 digest of a canonical allow-listed
record. The record is built in memory; only the digest is printed. It
includes the selected Chromium executable's resolved path, content and
validated version, the known kit launcher's content when present, and
Chromium resource files next to the selected executable. Font identity
includes the resolved directory and its font files; `DejaVuSans.ttf` is
required. Repository fixture sources are covered by the gate's tree;
`SKEIN_UI_EXAMPLE_BUNDLE` is built by the gate and is not caller evidence.

Compiler identity includes `rustc -vV`, Cargo's version, resolved compiler,
Cargo, rustdoc and configured wrapper executable contents, plus rustup's
resolved tools when the selected commands are shims. Installed
toolchain resolution precedes shim execution; validation does not install
a missing toolchain. Toolchain selection and semantic flag values are
hashed, never logged. Empty wrapper environment overrides retain their
Cargo meaning: disabling that wrapper.

Cargo configuration is read from Cargo home and `.cargo/config` (preferred
when both names exist) or `.cargo/config.toml` in ancestors through the
working directory, in increasing precedence. The record includes only
`build`'s compiler, wrapper, target, rustflags, rustdocflags and incremental
fields, target rustflags/rustdocflags/linker/runner fields, profile and
unstable settings, and `[env]` entries for the reviewed semantic environment
names. Arrays concatenate and scalars from nearer configurations win.
Inherited `PATH`, `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, rustdoc flags,
compiler and wrapper selections, `RUSTUP_TOOLCHAIN`, `RUSTC_BOOTSTRAP`,
`CARGO_BUILD_*` semantic fields,
`CARGO_PROFILE_*` and target flag/linker/runner variables are represented.
Configured wrapper, linker and runner executables are hashed when selected.
Unavailable target commands are recorded as unavailable; Cargo determines
whether that target needs them.
Lane job and test-thread counts are not semantic build fields.

The policy includes the kit's helper, `land`, `heavy`,
`heavy-recovery.py` when present, `kit.sh`, and the
repository's `scripts/check.sh` implementation contents. The policy also
hashes the Bash executable, gate command (`SKEIN_GATE_COMMAND`, set by
`land`) and its first executable when resolvable without evaluating shell
syntax. The cache key also includes the gate command and tree. Changing helper code invalidates
policy even when the checked tree differs only in documentation.

## 3. Evidence lifecycle

`land` prepares after each rebase and before either exact-tree lookup or
`Gate-Tree` reuse. A cache record has exactly three lines: the format
`gate-evidence-v2`, the policy identity, and the result summary. Its
filename hashes tree, gate command and policy. A prior tree's trailer must
name the same policy, its cache record must validate, and its tree must
remain available for the documentation-only diff. Writes use a
private temporary file and atomic rename. `--docs` records no gate policy
and does not produce gate evidence.

The gate admission path calls `gate_policy_prepare` before queueing and
records `SKEIN_GATE_POLICY_ID` with its task evidence. The queued gate
must retain the validated exports, including `SKEIN_GATE_COMMAND` when
set. Admission and payload preparation must reject a supplied identity that
differs from their computed identity. Revalidation must fail closed if the
policy changes during a run; `land` checks again on cache hits as well.
This identity is runtime evidence, separate from queue ordering and
transport policy.

## 4. Bounds and tests

This is a declared runtime contract, not a hermetic build. It does not
hash the operating system, shared libraries, every executable reachable
from arbitrary shell commands, system font fallback, or arbitrary
launchers' transitive dependencies. Known packaged Chromium resources
are hashed; an explicitly supplied other launcher is identified by its
own content and reported version. Cargo CLI `--config`, custom build
scripts' arbitrary inputs, and non-allow-listed `[env]` values are outside
the semantic configuration model. Cargo configuration `include` directives
are refused rather than leaving included semantic settings unidentified. The gate command and repository tree
still distinguish changes to declared commands and fixture sources.
Configuration changes in the selected fields can conservatively invalidate
evidence even when a higher-precedence setting masks them. Resources can
change after validation; pre/post checks detect persistent changes but do
not lock installations or protect against changes restored between checks.

Disposable local Git fixtures use fake runtimes and a fake heavy boundary.
They check exact and documentation-only reuse, runtime content and version
changes, semantic flags/configuration and wrapper changes, helper changes,
legacy refusal, export propagation, and mutation during a gate. A separate
lightweight validation uses the installed Chromium 147 executable, without
launching a browser session, building crates or running the full gate.
