use crate::Limits;
use crate::tests::fixtures::LIMITS;
use crate::{Act, Binding, BoundAct, Owner, number};
use crate::{Builder, NodeId, Role, State};

use alloc::boxed::Box;
use skein_lib::Token;

const MISSING: NodeId = NodeId(0); // no displayed node

const PAGE: NodeId = NodeId(1); // page

const BUTTON: NodeId = NodeId(2); // page, button

const FIELD: NodeId = NodeId(3); // page, button, field

const AREA: NodeId = NodeId(4); // page, button, field, area

const CHECK: NodeId = NodeId(5); // page, button, field, area, check

const LINK: NodeId = NodeId(6); // page, button, field, area, check, link

const MENU_ITEM: NodeId = NodeId(7); // page, button, field, area, check, link, menu item

fn bound_tree() -> crate::Tree {
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
        if role == Role::TextArea {
            builder.state(State::SubmitsOnEnter);
        }
        builder.close();
    }
    builder.close();
    let mut tree = builder.finish();
    for index in 0..tree.len() {
        tree.nodes.get_mut(index).expect("displayed node").id = NodeId(index.checked_add(1).expect("small tree"));
    }
    tree
}

#[test]
fn each_shell_act_resolves_by_node_identity_and_links_name_their_owner_address() {
    let tree = bound_tree();
    let binding = Binding { owner: Owner { number: 2 }, token: Token::new(7) };
    assert_eq!(tree.act(BUTTON, Act::Press), Some(BoundAct { binding, act: Act::Press }));
    assert_eq!(
        tree.act(FIELD, Act::Input { value: Box::from(b"42".as_slice()) }),
        Some(BoundAct { binding, act: Act::Input { value: Box::from(b"42".as_slice()) } })
    );
    assert_eq!(tree.act(AREA, Act::Submit), Some(BoundAct { binding, act: Act::Submit }));
    assert_eq!(tree.act(CHECK, Act::Toggle), Some(BoundAct { binding, act: Act::Toggle }));
    assert_eq!(tree.act(LINK, Act::Press), Some(BoundAct { binding, act: Act::Press }));
    assert_eq!(tree.act(MENU_ITEM, Act::Press), Some(BoundAct { binding, act: Act::Press }));
    assert_eq!(tree.address(LINK), Some(binding));
    assert!(tree.address(BUTTON).is_none());
    assert!(tree.act(PAGE, Act::Press).is_none());
    assert!(tree.act(MISSING, Act::Press).is_none());
    // The next displayed tree has removed the button; its old id resolves to nothing.
    let empty = Builder::new(&Limits { nodes: 1, ..LIMITS }).finish();
    assert!(empty.act(BUTTON, Act::Press).is_none());
    assert!(empty.address(LINK).is_none());
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
