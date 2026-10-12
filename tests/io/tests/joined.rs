//! Joined sockets with both owners in one process (io.md, 3.5 and 8).
use skein_io_world::joined::{closing, names_full, taken, unbound};
use skein_sim::Config;
const SEEDS: [u64; 3] = [1, 2, 3];

#[test]
fn a_connect_to_an_unbound_name_is_refused() {
    for seed in SEEDS {
        let outcome = unbound(seed, Config::chaos()).run();
        assert!(outcome.trace.is_empty(), "seed {seed}: no kernel submission");
    }
}

#[test]
fn a_second_listener_on_a_taken_name_fails_with_other() {
    for seed in SEEDS {
        assert!(taken(seed, Config::chaos()).run().trace.is_empty());
    }
}

#[test]
fn named_listens_past_the_names_limit_are_refused_busy() {
    for seed in SEEDS {
        assert!(names_full(seed, Config::chaos()).run().trace.is_empty());
    }
}

#[test]
fn a_closing_listener_refuses_its_waiting_connectors() {
    for seed in SEEDS {
        assert!(closing(seed, Config::chaos()).run().trace.is_empty());
    }
}
