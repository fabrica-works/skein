# Rebuilding the fleet's kache

`git-checkout-rev.patch` is commit `0f4075c69cd1998242b4747c3a125f89896a8b64`
on upstream v1.2.2, `96052b4842239bb216f6c4812054203c21550788`. It changes
four files in [kache](https://github.com/kunobi-ninja/kache), whose source
and licenses are fetched with the build. Its behaviour and verification
are [build-cache.md, section 4](../../../docs/foundation/build-cache.md).

Run from a skein worktree, naming a new absolute scratch directory:

```sh
scripts/dev/kache/build "$HOME/src/rust/tmp/kache-rebuild"
```

The script fetches that exact base, applies the patch, checks the musl
1.2.5 source archive's SHA-256, and queues the header generation and Cargo
build together through `heavy`. It uses Rust 1.95.0's musl target, clang
with musl headers, and Rust's bundled musl libc and `rust-lld`, producing
a static x86_64 Linux binary at `out/kache` in the scratch directory.
The machine must already have Rust 1.95.0 with the
`x86_64-unknown-linux-musl` target, clang, make, ar, readelf, curl and git;
the script installs no tools or system packages.

The scratch directory contains its own Cargo home and target; the build
runs with the rustc wrapper disabled. It neither installs the output nor
starts a daemon. Remove that scratch directory after keeping the output
and its `out/SHA256SUMS` if needed.

The script prints the digest and whether it matches the deployed
`93f8551e8f1deedfab30118957d5450e2204b4a09a9cd951e092426c096839a3`.
A different digest is a rebuild result, not permission to change
`kache-setup`'s pin: the director must verify and place any new binary.

Verified on hetzner2, 2026-10-24: release build passed in 7m56s; the output
reported `kache 1.2.2-gitrev.1` and had no dynamic interpreter. Its SHA-256
was `d9ba88fb73359c05625ef19a6734082a7152b4c7198c7547a20dd9ba47f53309`.
This recipe did not reproduce the deployed digest; byte-for-byte
reproducibility remains open. The installed binary and pin were kept.
