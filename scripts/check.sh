#!/usr/bin/env bash
# What CI runs: formatting, lints as errors, then the focused, fuzzy and
# browser suites in full (testing-strategy.md, section 8; testing.md, 6).
set -eu

cd "$(dirname "$0")/.."

# gate-policy.md, 1: the gate validates the same runtime as admission.
# shellcheck source=dev/gate-policy.sh
source "${SKEIN_GATE_POLICY_HELPER:-scripts/dev/gate-policy.sh}"
expected_policy=${SKEIN_GATE_POLICY_ID:-}
gate_policy_prepare || exit 2
if [[ -n $expected_policy && $expected_policy != "$SKEIN_GATE_POLICY_ID" ]]; then
    echo "gate-policy: admission policy differs from the gate runtime" >&2
    exit 2
fi

python3 -I -B -m unittest discover -t scripts/lab -s scripts/lab/tests/unit
cargo fmt --all --check
# testing-strategy.md 10: A setup written once; Named fixtures and sizes.
mkdir -p target
rustc --edition=2024 -D warnings scripts/check-contracts.rs -o target/check-contracts
target/check-contracts
# testing-strategy.md 10: A setup written once; The setup the assertion reads; Named fixtures and sizes.
cargo run --locked --target-dir target/recovery-shape --manifest-path scripts/recovery-shape/Cargo.toml -- --controls crates/skein-forge-client/src/tests/recovery.rs crates/skein-forge-client/src/tests/fixtures/recovery.rs
# testing-strategy.md 10: One behaviour per test, in the four sampled plan tests.
cargo run --locked --target-dir target/recovery-shape --manifest-path scripts/recovery-shape/Cargo.toml -- --plan-shapes --controls crates/skein-forge/src/forgejo/plans/tests/edges.rs
# testing-strategy.md 10: sampled 03.3 behaviours and capacities.
cargo run --locked --target-dir target/recovery-shape --manifest-path scripts/recovery-shape/Cargo.toml -- --connection-shapes --controls crates/skein-forge-connection/src/tests/connection.rs crates/skein-forge-connection/src/limits/tests.rs
# testing-strategy.md 10: Named fixtures and sizes, in the two sampled listing tests.
cargo run --locked --target-dir target/recovery-shape --manifest-path scripts/recovery-shape/Cargo.toml -- --listing-sizes --controls crates/skein-forge/src/forgejo/plans/tests/calls.rs
cargo clippy --workspace --all-targets -- -D warnings
# The portable libraries and the browser-only halves on their shipping target.
cargo check --target wasm32-unknown-unknown -p skein-lib -p skein-codec -p skein-http
cargo clippy --target wasm32-unknown-unknown -p skein-ui -p skein-ui-web \
    -p skein-ui-example-view -p skein-ui-example-shell-web -- -D warnings
scripts/taste.sh crates/skein-tls tests/tls
# The TLS machine on its own: the workspace's build unifies rustls's std,
# which tests/tls turns on, and would hide a use of it.
cargo check -p skein-tls
# testing-strategy.md, section 8: nextest starts each test directly.
export NEXTEST_DOUBLE_SPAWN=0
cargo nextest run --workspace
cargo nextest run --workspace --profile fuzzy
# The browser tier uses a dev-profile module under the server's strict policy.
cargo build --target wasm32-unknown-unknown -p skein-ui-example-shell-web
cargo run -p skein-ui-bundle -- \
    --module target/wasm32-unknown-unknown/debug/skein_ui_example_shell_web.wasm \
    --name skein_ui_example_shell_web --prefix /bundle/ --stylesheet examples/ui/shell-web/notes.css --out target/ui-example-bundle \
    --noscript 'Notes needs WebAssembly and scripts.'
cargo build -p skein-ui-example-server
SKEIN_UI_EXAMPLE_BUNDLE="$(pwd)/target/ui-example-bundle"
export SKEIN_UI_EXAMPLE_BUNDLE
cargo nextest run --workspace --profile browser
