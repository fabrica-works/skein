#!/bin/sh
# The tiers the gate leaves out that main must keep green: `land` runs this
# after each code landing, through heavy --gate, and a failure goes to the
# director (owners.md, 4.5). Today the TLS every-byte sweep, nextest's
# exhaustive profile (forge-connection.md, 10), once main has it.
set -eu
cd "$(dirname "$0")/.."
python3 -I -B -m unittest discover -t scripts/lab -s scripts/lab/tests/process -v
if ! grep -q '^\[profile\.exhaustive\]' .config/nextest.toml; then
    echo "after-landing: no [profile.exhaustive] in .config/nextest.toml; nothing to run"
    exit 0
fi
exec cargo nextest run --workspace --profile exhaustive
