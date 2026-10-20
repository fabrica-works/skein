use skein_forge_setup_world::memory::{self, Kinds};
#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;

#[test]
fn forgejo_plaintext_host_fits() {
    check(Kinds::Forgejo, false, 0);
}
#[test]
fn forgejo_tls_host_fits() {
    check(Kinds::Forgejo, true, 0);
}
#[test]
fn github_plaintext_host_fits() {
    check(Kinds::GitHub, false, 8);
}
#[test]
fn github_tls_host_fits() {
    check(Kinds::GitHub, true, 8);
}
#[test]
fn mixed_host_fits() {
    check(Kinds::Both, true, 8);
}
fn check(kinds: Kinds, tls: bool, states: u32) {
    let full = memory::full_heap(kinds, tls);
    assert_eq!((full.runs, full.connections, full.states), (2, 2, states));
    assert!(full.owned > 0 && full.owned <= full.bound);
    println!(
        "{kinds:?}, tls={tls}: owned={}, bound={}, margin={}, runs={}, connections={}, states={}",
        full.owned,
        full.bound,
        full.bound - full.owned,
        full.runs,
        full.connections,
        full.states
    );
}
