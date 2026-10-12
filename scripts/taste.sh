#!/bin/sh
# Check the directories named by the gate.
set -eu

# tls-cleanup's README Pulse words.
words='actual|actually|genuine|genuinely|authentic|authentically|honest|truly'
status=0
if rg -n -i -- "(^|[^[:alnum:]])($words)([^[:alnum:]]|$)" "$@"; then
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
