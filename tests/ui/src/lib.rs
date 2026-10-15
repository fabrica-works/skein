//! Pages filled to their configured text and node limits (ui.md, 17).
use skein_ui::{Builder, Limits, Owner, Role, Run, Style};

/// Build every node with its full text capacity; the tree consumes all nodes.
/// The fixture's byte budget must cover every node's per-node text capacity.
pub fn full_page(limits: &Limits, builder: &mut Builder) {
    let bytes = vec![b'x'; usize::try_from(limits.text_per_node).expect("fixture bytes fit usize")];
    builder.open(Role::Page);
    builder.name(&bytes);
    for _ in 1..limits.nodes {
        builder.open(Role::Text);
        for _ in 0..limits.text_per_node {
            builder.run(Run::Plain, b"x");
        }
        for token in 0..limits.styles_per_node {
            builder.style(Style {
                owner: Owner { number: 1 },
                token: u16::try_from(token).expect("fixture tokens fit u16"),
            });
        }
        builder.close();
    }
    builder.close();
}
