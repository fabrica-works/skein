#[path = "support/look_world.rs"]
mod look_world;
#[test]
fn look_reads_hold_the_machine_contracts() {
    look_world::run(24);
}
