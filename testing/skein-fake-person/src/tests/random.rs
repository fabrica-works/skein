use super::*;
use std::collections::BTreeSet;

#[test]
fn random_people_replay_cover_enabled_controls_and_reject_unavailable_nodes() {
    random_people_replay_the_same_seed_over_the_same_trees();
    a_sweep_acts_on_every_bound_node_and_reaches_fields_composers_and_history();
    random_people_never_act_on_unbound_disabled_or_unshown_nodes();
}

const BUTTON: NodeId = NodeId(2); // page, button
const FIELD: NodeId = NodeId(3); // page, button, field
const COMPOSER: NodeId = NodeId(4); // page, button, field, composer
const CHECK: NodeId = NodeId(5); // page, button, field, composer, check
const LINK: NodeId = NodeId(6); // page, button, field, composer, check, link

fn random_tree() -> View {
    build(|b| {
        b.open(Role::Page);
        b.open(Role::Button);
        b.name(b"Accept");
        b.bind(binding(1));
        b.close();
        b.open(Role::TextField);
        b.name(b"Subject");
        b.bind(binding(2));
        b.close();
        b.open(Role::TextArea);
        b.name(b"Reply");
        b.bind(binding(3));
        b.state(State::SubmitsOnEnter);
        b.close();
        b.open(Role::Check);
        b.name(b"Checked");
        b.bind(binding(4));
        b.close();
        b.open(Role::Link);
        b.name(b"Task");
        b.link(binding(5));
        b.close();
        b.open(Role::TextArea);
        b.name(b"Disabled");
        b.bind(binding(6));
        b.state(State::Disabled);
        b.close();
        b.open(Role::Button);
        b.name(b"Unbound");
        b.close();
        b.close();
    })
}

fn random_people_replay_the_same_seed_over_the_same_trees() {
    let first = random_tree();
    let second = fixture();
    for seed in 0..32 {
        let mut a = RandomPerson::new(seed, 32);
        let mut b = RandomPerson::new(seed, 32);
        for at in 0..64 {
            let tree = if at % 2 == 0 { first.tree() } else { second.tree() };
            assert_eq!(a.next(tree), b.next(tree), "seed {seed} at {at}");
        }
    }
}

#[derive(Default)]
struct Navigation {
    back: bool,
    reload: bool,
}

#[derive(Default)]
struct Sweep {
    pressed: BTreeSet<NodeId>,
    inputs: BTreeSet<NodeId>,
    submitted: BTreeSet<NodeId>,
    toggled: BTreeSet<NodeId>,
    alphabet: BTreeSet<u8>,
    navigation: Navigation,
    empty: bool,
    full: bool,
}
impl Sweep {
    fn observe(&mut self, tree: &skein_ui::Tree, doing: Option<RandomDoing>) {
        match doing {
            Some(RandomDoing::Act { node, act }) => {
                let control = tree.find(node).expect("visible control");
                assert!(TreeFace::new(tree).allows(node, &act));
                match act {
                    Act::Press => {
                        self.pressed.insert(node);
                    }
                    Act::Input { value } => {
                        assert!(value.len() <= 32);
                        self.empty |= value.is_empty();
                        self.full |= value.len() == 32;
                        self.inputs.insert(node);
                        self.alphabet.extend(value.iter().copied());
                    }
                    Act::Submit => {
                        self.submitted.insert(node);
                        assert!(control.states.as_slice().contains(&State::SubmitsOnEnter));
                    }
                    Act::Drawing { .. } => panic!("the random person never makes drawing acts"),
                    Act::Toggle => {
                        self.toggled.insert(node);
                        assert_eq!(control.role, Role::Check);
                    }
                }
            }
            Some(RandomDoing::Back) => self.navigation.back = true,
            Some(RandomDoing::Reload) => self.navigation.reload = true,
            None => unreachable!("tree has bound nodes"),
        }
    }
}
fn a_sweep_acts_on_every_bound_node_and_reaches_fields_composers_and_history() {
    let view = random_tree();
    let mut sweep = Sweep::default();
    for seed in 0..32 {
        let mut person = RandomPerson::new(seed, 32);
        for _ in 0..64 {
            sweep.observe(view.tree(), person.next(view.tree()));
        }
    }
    let acted: BTreeSet<_> = sweep
        .pressed
        .union(&sweep.inputs)
        .copied()
        .chain(sweep.submitted.iter().copied())
        .chain(sweep.toggled.iter().copied())
        .collect();
    assert_eq!(acted, BTreeSet::from([BUTTON, FIELD, COMPOSER, CHECK, LINK]));
    assert_eq!(sweep.inputs.len(), 2);
    assert_eq!(sweep.submitted.len(), 1);
    assert_eq!(sweep.toggled.len(), 1);
    assert!(sweep.navigation.back && sweep.navigation.reload && sweep.empty && sweep.full);
    for byte in b" 0123456789#*_`[]()>-+.!\n" {
        assert!(sweep.alphabet.contains(byte));
    }
}

fn random_people_never_act_on_unbound_disabled_or_unshown_nodes() {
    let view = build(|b| {
        b.open(Role::Page);
        b.open(Role::Button);
        b.name(b"Unbound");
        b.close();
        b.open(Role::TextArea);
        b.name(b"Disabled");
        b.bind(binding(1));
        b.state(State::Disabled);
        b.close();
        b.close();
    });
    let empty = skein_ui::Tree::new(0);
    let unshown = {
        let mut b = Builder::new(&LIMITS);
        b.open(Role::Button);
        b.bind(binding(1));
        b.close();
        b.finish()
    };
    let mut person = RandomPerson::new(1, 0);
    for tree in [view.tree(), &empty, &unshown] {
        for _ in 0..128 {
            assert!(matches!(person.next(tree), None | Some(RandomDoing::Back | RandomDoing::Reload)));
        }
    }
    let view = random_tree();
    for _ in 0..128 {
        if let Some(RandomDoing::Act { act: Act::Input { value }, .. }) = person.next(view.tree()) {
            assert!(value.is_empty());
        }
    }
}

const ROLE_NAMES: &[(Role, &str)] = &[
    (Role::Page, "page"),
    (Role::Banner, "banner"),
    (Role::Navigation, "navigation"),
    (Role::Main, "main"),
    (Role::Complementary, "complementary"),
    (Role::ContentInfo, "content info"),
    (Role::Form, "form"),
    (Role::Search, "search"),
    (Role::Region, "region"),
    (Role::Heading { level: 2 }, "heading"),
    (Role::Paragraph, "paragraph"),
    (Role::Text, "text"),
    (Role::List, "list"),
    (Role::Item, "item"),
    (Role::Table, "table"),
    (Role::Row, "row"),
    (Role::Cell, "cell"),
    (Role::ColumnHeader, "column header"),
    (Role::RowHeader, "row header"),
    (Role::Select, "select"),
    (Role::Option, "option"),
    (Role::Term, "term"),
    (Role::Description, "description"),
    (Role::Button, "button"),
    (Role::Link, "link"),
    (Role::TextField, "text field"),
    (Role::TextArea, "text area"),
    (Role::Choice, "choice"),
    (Role::ChoiceGroup, "choice group"),
    (Role::Check, "check"),
    (Role::Dialog, "dialog"),
    (Role::Status, "status"),
    (Role::Alert, "alert"),
    (Role::Log, "log"),
    (Role::Tree, "tree"),
    (Role::TreeItem, "tree item"),
    (Role::Tabs, "tabs"),
    (Role::Tab, "tab"),
    (Role::Progress, "progress"),
    (Role::Time, "time"),
    (Role::Code, "code"),
    (Role::Quote, "quote"),
    (Role::Separator, "separator"),
    (Role::Menu, "menu"),
    (Role::MenuItem, "menu item"),
    (Role::Disclosure, "disclosure"),
    (Role::Image, "image"),
    (Role::Figure, "figure"),
    (Role::Slot, "slot"),
    (Role::Drawing, "drawing"),
];

#[test]
fn each_role_has_its_own_name() {
    let failures: Vec<_> = ROLE_NAMES
        .iter()
        .filter_map(|(role, expected)| {
            let actual = crate::tree::role_name(*role);
            (actual != *expected).then_some((*role, *expected, actual))
        })
        .collect();
    assert_eq!(failures, []);
}

const DRAWING_BUTTON: NodeId = NodeId(3); // page, drawing, button
fn page_with_bound_drawing_and_button() -> View {
    build(|b| {
        b.open(Role::Page);
        b.drawing(DrawingKind { owner: Owner { number: 1 }, kind: 1 }, b"Chart", b"data", 32);
        b.bind(binding(1));
        b.close();
        b.open(Role::Button);
        b.bind(binding(2));
        b.close();
        b.close();
    })
}

#[test]
fn the_random_person_never_acts_on_a_drawing() {
    let view = page_with_bound_drawing_and_button();
    let mut person = RandomPerson::new(7, 4);
    let mut acted = false;
    for _ in 0..200 {
        if let Some(RandomDoing::Act { node, act }) = person.next(view.tree()) {
            assert_eq!(RandomDoing::Act { node, act }, RandomDoing::Act { node: DRAWING_BUTTON, act: Act::Press });
            acted = true;
        }
    }
    assert!(acted);
}

const OPTION: NodeId = NodeId(3); // page, select, option
fn page_with_option(disabled: bool) -> View {
    build(|b| {
        b.open(Role::Page);
        b.open(Role::Select);
        if disabled {
            b.state(State::Disabled);
        }
        b.open(Role::Option);
        b.name(b"Daily");
        b.bind(binding(1));
        b.close();
        b.close();
        b.close();
    })
}

#[test]
fn the_random_person_presses_an_option() {
    let view = page_with_option(false);
    let mut person = RandomPerson::new(7, 4);
    let mut acted = false;
    for _ in 0..200 {
        if let Some(RandomDoing::Act { node, act }) = person.next(view.tree()) {
            assert_eq!(RandomDoing::Act { node, act }, RandomDoing::Act { node: OPTION, act: Act::Press });
            acted = true;
        }
    }
    assert!(acted);
}

#[test]
fn the_random_person_never_acts_on_an_option_of_a_disabled_select() {
    let view = page_with_option(true);
    let mut person = RandomPerson::new(7, 4);
    for _ in 0..200 {
        assert!(matches!(person.next(view.tree()), None | Some(RandomDoing::Back | RandomDoing::Reload)));
    }
}

#[test]
fn the_random_person_toggles_a_choice() {
    let view = page_with_control(Role::Choice, b"Daily");
    let mut person = RandomPerson::new(7, 4);
    let mut acted = false;
    for _ in 0..200 {
        if let Some(RandomDoing::Act { node, act }) = person.next(view.tree()) {
            assert_eq!(RandomDoing::Act { node, act }, RandomDoing::Act { node: CONTROL, act: Act::Toggle });
            acted = true;
        }
    }
    assert!(acted);
}
