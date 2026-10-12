# Baseline

2026-10-12, skein at `73c900b`. The numbers this plan starts from, and the
commands that count them, run from skein's primary checkout. The
retrospective runs the same commands at the end (development.md,
section 15).

## 1. Size and words

```sh
find crates/skein-tls tests/tls -name '*.rs' | xargs wc -l | awk '$2 != "total" && $1 > 800'
rg -i -c '(^|[^[:alnum:]])(actual|actually|genuine|genuinely|authentic|authentically|honest|truly)([^[:alnum:]]|$)' crates/skein-tls tests/tls
```

- 9,913 lines of Rust in all (10,114 with the manifests and fixtures);
  production `crates/skein-tls/src` without its tests, about 2,550.
- Over 800 lines: `client.rs` (1,114), `client/native.rs` (1,004),
  `tests/tls/src/world.rs` (1,043).
- The words of the Pulse list, in names too: 331 lines, nearly all in
  the native face's files (`client/native*`, `tests/tls/src/native*`,
  `tests/tls/tests/native_*`, `fuzzy_native.rs`); one in
  `tests/tls/Cargo.toml`. Counting "real", "exact", "whole" and
  "complete" too, as whole words: 494, about 427 in the native face's
  files.

## 2. Test names

```sh
rg -U --no-filename -o '#\[test\](\s*#\[[^\n]*\])*\s*fn ([a-z0-9_]+)' -r '$2' crates/skein-tls/src tests/tls |
    awk -F_ '{ print NF }' | sort -n |
    awk '{ a[NR] = $1 } END { print "tests", NR, "median", a[int((NR + 1) / 2)], "max", a[NR] }'
rg -U --no-filename -o '#\[test\](\s*#\[[^\n]*\])*\s*fn ([a-z0-9_]+)' -r '$2' crates/skein-tls/src tests/tls |
    awk -F_ 'NF > 12'
```

- 116 tests; the median name is 10 words, the longest 15; 19 names are
  over 12 words.

## 3. Test functions over 40 lines

```sh
for f in $(rg -l '#\[test\]' crates/skein-tls/src tests/tls); do
    awk -v f="$f" '
        /#\[test\]/ { t = 1; next }
        t && /^[[:space:]]*fn [a-z0-9_]+\(/ {
            match($0, /^[[:space:]]*/); ind = RLENGTH; name = $0
            sub(/^[[:space:]]*fn /, "", name); sub(/\(.*/, "", name)
            start = NR; t = 0; inside = 1; next
        }
        inside && match($0, /^[[:space:]]*\}$/) && RLENGTH - 1 == ind {
            if (NR - start + 1 > 40) print NR - start + 1, f, name
            inside = 0
        }' "$f"
done | sort -rn
```

- 7: `native_memory.rs` (63), `fake_peer.rs` (56), four in
  `native_output.rs` (53, 52, 47, 47), `fuzzy_world.rs`'s
  `exchanges_swept` (45).

Any function over a number of lines, for the 60-line guide of
`code-organization.md` (FILES are the files to look at, N the limit):

```sh
for f in FILES; do
    awk -v f="$f" -v max=N '
        /^[[:space:]]*(pub(\([^)]*\))? )?(const )?fn [a-z0-9_]+[<(]/ && !inside {
            match($0, /^[[:space:]]*/); ind = RLENGTH; name = $0
            sub(/^[[:space:]]*(pub(\([^)]*\))? )?(const )?fn /, "", name); sub(/[<(].*/, "", name)
            if ($0 ~ /;[[:space:]]*$/) next
            start = NR; inside = 1; next
        }
        inside && match($0, /^[[:space:]]*\}$/) && RLENGTH - 1 == ind {
            if (NR - start + 1 > max) print NR - start + 1, f, name
            inside = 0
        }' "$f"
done | sort -rn
```

## 4. Cost

```sh
cargo nextest run -p skein-tls -p skein-tls-world --profile measure -j 1
cargo nextest run -p skein-tls-world --profile measure -j 1 --ignore-default-filter -E 'binary(/^fuzzy_/)'
```

- Focused: 114 tests, 1.9 s on one thread; the longest is
  `memory::failures_and_closes_in_every_state`, 0.39 s.
- Fuzzy: 2 tests, 2.1 s; `exchanges_swept` 1.97 s.
- Test cost is not a target of this plan (`decisions.md`, 8).

## 5. Duplication

- The native face restates about 85 lines of the classic client's
  (findings B-1, N-3); `State::TransportClosed` is written out 24 times
  in `native.rs` (B-3).
- Helpers defined in more than one file of the world crate: `SMALL` and
  `LARGE`, the meter's `check` and `finish`, the first record's
  corruption, the lower `Output` proxy, the ready native pair (4 times),
  `Limits { read: 16, send: 16, .. }` (6 times), the record length read
  from a header (9 sites) (B-8, B-9, B-10, T-9).
