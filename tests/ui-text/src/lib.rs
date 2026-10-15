use skein_lib::{Queue, Token};
use skein_ui::{Binding, Builder, Limits, Owner, Role, State, Tree};

pub const LIMITS: Limits = Limits {
    nodes: 160,
    depth: 12,
    text_per_node: 128,
    text_bytes: 2048,
    drawing_bytes: 8,
    styles_per_node: 2,
    patches: 960,
    bindings: 32,
    slots: 2,
    markdown_depth: 2,
};
#[must_use]
pub fn binding() -> Binding {
    Binding { owner: Owner { number: 1 }, token: Token::new(1) }
}
pub fn page(main: fn(&mut Builder)) -> Tree {
    let mut b = Builder::new(&LIMITS);
    b.open(Role::Page);
    b.open(Role::Banner);
    b.close();
    b.open(Role::Main);
    main(&mut b);
    b.close();
    b.close();
    let mut tree = b.finish();
    let mut patches = Queue::with_capacity(LIMITS.patches);
    skein_ui::diff(&Tree::new(LIMITS.nodes), &mut tree, &LIMITS, &mut 1, &mut patches);
    tree
}
pub fn button(b: &mut Builder, name: &[u8], enabled: bool) {
    b.open(Role::Button);
    b.name(name);
    b.bind(binding());
    if !enabled {
        b.state(State::Disabled);
    }
    b.close();
}
