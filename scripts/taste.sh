#!/bin/sh
# Check the directories named by the gate: no word the plan forbids, in
# prose or in names (snake_case, CamelCase, with digits), and no Rust file
# over 800 lines. Without a directory, it refuses: it never checks the
# whole repository by default.
set -eu
if [ "$#" -eq 0 ]; then
    echo "usage: scripts/taste.sh DIRECTORY..." >&2
    exit 2
fi

# tls-cleanup's README Pulse words.
words='actual|actually|genuine|genuinely|authentic|authentically|honest|truly'
upper=$(printf '%s\n' "$words" | tr '[:lower:]' '[:upper:]')
capital=$(printf '%s\n' "$words" | awk -F'|' -v OFS='|' '{
    for (i = 1; i <= NF; i++) $i = toupper(substr($i, 1, 1)) substr($i, 2); print }')
# A word stands alone, or starts a name's part: after a lower-case letter
# only when capitalised; before anything but a lower-case letter.
pattern="((^|[^[:alpha:]])($words|$upper)|(^|[^[:upper:]])($capital))([^a-z]|\$)"

status=0
if rg -n -- "$pattern" "$@"; then
    status=1
else
    result=$?
    [ "$result" -eq 1 ] || exit "$result"
fi

over_limit=$(find "$@" -type f -name '*.rs' -exec wc -l {} \; | awk '$1 > 800')
if [ -n "$over_limit" ]; then
    printf '%s\n' "$over_limit"
    status=1
fi
exit "$status"
