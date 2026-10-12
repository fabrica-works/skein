# Later

What the audit found and this plan does not do (`findings/audit.md`).

| Item | Design | When |
|---|---|---|
| The native face's `waiting()` has no state for a close that waits for the owner's `Closed`; it answers `Close`, defined for a failure (B-5) | tls.md, 3.1 and 3.6 | with the native face's first user |
| Requests after a failure on the native face: dropped or asserted (N-9) | tls.md, 7 | with the native face's first user |
| session.rs's `process` and `encrypt` nest five deep (B-6) | none | the next plan that touches the session |
| Tests that run more seeds and bytes than their assertions need: exchange.rs's 70 worlds for 5 waits, 100,000-byte payloads, a configuration built per run (T-8) | testing-strategy.md, 8 | when the suites near their budgets |
| `tests/tls/tests/fake_peer.rs` tests skein-fake-peers' TLS face, with 6 dev-dependencies for it, and a 56-line test; it belongs in testing/skein-fake-peers (T-12) | tls.md, 5 | a plan that touches skein-fake-peers |
| `fuzzy_native.rs` asserts nothing its focused twin does not: it does not check that short and reordered completions happened (T-13) | testing-strategy.md, 3 | the native face's first user |
| `exchanges_swept`, 45 lines | tls.md, 5 | with T-8 |
| smith's agent shell builds its root store by hand and calls `Config::new`, skipping `from_der`'s key filtering (smith's `crates/smith-agent-shell/src/config.rs`) | tls.md, 3.4 | a smith plan |
| lib.md, 7.1 and io.md, 3.3 are written in the words tls.md, 3.6 had | lib.md, io.md | a design pass |
| `scripts/taste.sh` over every crate of skein, then smith, jig and temper | development.md, 4.5 | after this run's retrospective |
| `plan check` checks the primary checkouts of every repository, not only the plan's and those upstream of it; this run sets `DEV_REPOS=skein` instead | development.md, 6 | the kit's next change |
| `plan check` does not check that `origin` accepts a push: skein's HTTPS `origin` had no credentials on carbon-forge, and every `land` would have failed (exit 7); its push URL was set to SSH before this run | development.md, 6 | the kit's next change |
