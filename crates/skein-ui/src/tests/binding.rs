use crate::Limits;
use crate::tests::fixtures::LIMITS;
use crate::{Act, Binding, BoundAct, Owner, number};
use crate::{Builder, NodeId, Role};

use alloc::boxed::Box;
use skein_lib::Token;

#[test]
fn each_shell_act_resolves_by_node_identity_and_links_name_their_owner_address() {
    let mut builder = Builder::new(&Limits { bindings: 6, ..LIMITS });
    builder.open(Role::Page);
    for role in [Role::Button, Role::TextField, Role::TextArea, Role::Check, Role::Link, Role::MenuItem] {
        builder.open(role);
        let binding = Binding { owner: Owner { number: 2 }, token: Token::new(7) };
        if role == Role::Link {
            builder.link(binding);
        } else {
            builder.bind(binding);
        }
        builder.close();
    }
    builder.close();
    let mut tree = builder.finish();
    for index in 0..tree.len() {
        tree.nodes.get_mut(index).expect("displayed node").id = NodeId(index.checked_add(1).expect("small tree"));
    }
    let binding = Binding { owner: Owner { number: 2 }, token: Token::new(7) };
    assert_eq!(tree.act(NodeId(2), Act::Press), Some(BoundAct { binding, act: Act::Press }));
    assert_eq!(
        tree.act(NodeId(3), Act::Input { value: Box::from(b"42".as_slice()) }),
        Some(BoundAct { binding, act: Act::Input { value: Box::from(b"42".as_slice()) } })
    );
    assert_eq!(tree.act(NodeId(4), Act::Submit), Some(BoundAct { binding, act: Act::Submit }));
    assert_eq!(tree.act(NodeId(5), Act::Toggle), Some(BoundAct { binding, act: Act::Toggle }));
    assert_eq!(tree.act(NodeId(6), Act::Press), Some(BoundAct { binding, act: Act::Press }));
    assert_eq!(tree.act(NodeId(7), Act::Press), Some(BoundAct { binding, act: Act::Press }));
    assert_eq!(tree.address(NodeId(6)), Some(binding));
    assert!(tree.address(NodeId(2)).is_none());
    assert!(tree.act(NodeId(1), Act::Press).is_none());
    assert!(tree.act(NodeId(0), Act::Press).is_none());
    // The next displayed tree has removed the button; its old id resolves to nothing.
    let empty = Builder::new(&Limits { nodes: 1, ..LIMITS }).finish();
    assert!(empty.act(NodeId(2), Act::Press).is_none());
    assert!(empty.address(NodeId(6)).is_none());
}

#[test]
fn numeric_input_is_checked_digits_only() {
    assert_eq!(number(b"0"), Some(0));
    assert_eq!(number(b"00042"), Some(42));
    assert_eq!(number(b"18446744073709551615"), Some(u64::MAX));
    for bytes in [b"".as_slice(), b"-1", b"+1", b" 1", b"1 ", b"1.0", b"1x", b"18446744073709551616", b"\xff"] {
        assert!(number(bytes).is_none(), "non-digits or overflow are refused");
    }
}

#[test]
fn an_act_on_a_drawing_past_its_bytes_is_dropped() {
    let mut view = crate::View::new(&LIMITS);
    let mut builder = view.builder();
    // Exercise admission through an ordinary binding: drawings remain unbound.
    builder.open(Role::Button);
    let binding = Binding { owner: Owner { number: 2 }, token: Token::new(1) };
    builder.bind(binding);
    builder.close();
    let mut out = skein_lib::Queue::with_capacity(LIMITS.patches);
    view.finish(builder, 1, &mut out);
    let id = view.tree().get(0).unwrap().id;
    let data = Box::from(*b"12345678");
    assert_eq!(
        view.act(id, Act::Drawing { data }),
        Some(BoundAct { binding, act: Act::Drawing { data: Box::from(*b"12345678") } })
    );
    assert_eq!(view.act(id, Act::Drawing { data: Box::from(*b"123456789") }), None);
}
