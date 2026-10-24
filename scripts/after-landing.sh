#!/bin/sh
# The tiers the gate leaves out that main must keep green: `land` runs this
# after each code landing, through heavy --gate, and a failure goes to the
# director (agents' roles/owners.md, 4.5). Today the TLS every-byte sweep, nextest's
# exhaustive profile (forge-connection.md, 10), once main has it.
set -eu
cd "$(dirname "$0")/.."
echo "after-landing: lab process tier skipped for the attended hetzner prototype (decision 273)"
if ! grep -q '^\[profile\.exhaustive\]' .config/nextest.toml; then
    echo "after-landing: no [profile.exhaustive] in .config/nextest.toml; nothing to run"
    exit 0
fi
exec cargo nextest run --workspace --profile exhaustive
