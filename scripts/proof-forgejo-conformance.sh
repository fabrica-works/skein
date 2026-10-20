#!/bin/sh
# Record both modes against the pinned binaries (fake-forge.md, 7 and 8).
set -eu
forgejo=
runner=
scratch=
out=
while [ "$#" -gt 0 ]; do
    [ "$#" -ge 2 ] || { echo "missing flag value" >&2; exit 2; }
    case "$1" in
        --forgejo) forgejo=$2 ;;
        --runner) runner=$2 ;;
        --scratch) scratch=$2 ;;
        --out) out=$2 ;;
        *) echo "unknown flag: $1" >&2; exit 2 ;;
    esac
    shift 2
done
[ -n "$forgejo" ] || { echo "--forgejo required" >&2; exit 2; }
[ -n "$runner" ] || { echo "--runner required for delivery CI" >&2; exit 2; }
[ -n "$scratch" ] || { echo "--scratch required" >&2; exit 2; }
[ -n "$out" ] || { echo "--out required" >&2; exit 2; }
cargo run -p forgejo-conformance -- --forgejo "$forgejo" --runner "$runner" \
    --scratch "$scratch/ordinary" --out "$out"
cargo run -p forgejo-conformance -- --forgejo "$forgejo" --runner "$runner" \
    --scratch "$scratch/component" --out "$out" --component
