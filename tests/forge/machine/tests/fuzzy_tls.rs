use skein_forge_machine_world::tls::{Sweep, tls_cuts};

#[test]
fn tls_faults_at_each_boundary_end_once() {
    for (written, cost, terminals, closed) in tls_cuts(7, Sweep::Boundaries) {
        assert_eq!(cost, written);
        assert_eq!((terminals, closed), (1, 1));
    }
}
