use skein_forge_machine_world::{
    fixtures,
    hooks::{self, Listener},
};
#[test]
fn a_delivery_split_at_every_point_is_answered_alike() {
    let post = hooks::push(b"main", 7);
    let mut bytes = post.head.into_vec();
    bytes.extend_from_slice(&post.body);
    for split in 0..=bytes.len() {
        let mut world = Listener::new(7, fixtures::hook_limits(), fixtures::hook_settings());
        let peer = world.accept();
        world.write(peer, &bytes[..split]);
        world.write(peer, &bytes[split..]);
        assert_eq!(world.status(peer), Some(204), "split {split}");
        assert_eq!(world.hints().len(), 1, "split {split}");
        assert!(world.hinted_after_answer(), "split {split}");
    }
}

#[test]
fn listener_faults_at_each_byte() {
    for (case, counts) in hooks::listener_faults(7).into_iter().enumerate() {
        assert_eq!(counts, [0, 0, 0, 0, 0, 1], "case {case}");
    }
}
