//! Native reads and output under short, reordered I/O (tls.md, 3.6).
use skein_lib::Duration;
use skein_sim::{Config, Faults};
use skein_tls::client::Version;
use skein_tls_world::native::{Case, conversation};

#[test]
fn independent_reads_and_output_settle_under_short_reordered_io() {
    let config = Config {
        buffer: 7,
        faults: Faults {
            latency: 500,
            latency_max: Duration::from_millis(2),
            short_send: 500,
            short_recv: 500,
            ..Faults::NONE
        },
        ..Config::calm()
    };
    let mut completed = 0;
    for seed in 1..=8 {
        for version in [Version::Tls12, Version::Tls13] {
            let outcome = conversation(seed, config, Case::Late, version);
            assert_eq!(outcome.server_plaintext, b"ping");
            assert_eq!(outcome.reply, b"ok");
            assert!(outcome.behind_both_reads);
            assert_eq!(outcome.closed, 1);
            completed += 1;
        }
    }
    assert_eq!(completed, 16, "every real conversation completed, no refusal substitutions");
}
