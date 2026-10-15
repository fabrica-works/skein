use super::*;
use skein_ui::Binding;
use skein_ui::Limits;
use skein_ui::{Builder, DrawingKind, Owner, Role, Run, State, View};

use skein_lib::{Queue, Token};

// Keep each scenario's fresh fixture and assertions, while sharing the test
// process startup cost within the kits' serial budget (skein's ui.md,
// section 17.1).
#[test]
fn tree_queries_find_regions_words_and_drawing_fallbacks() {
    finds_nested_regions_and_does_not_escape_their_subtrees();
    text_queries_join_runs_and_ignore_case();
    drawings_are_found_by_name_and_fallback_never_by_data();
}

#[test]
fn scripted_people_report_acts_failures_and_bounded_waits() {
    a_script_reports_shell_acts_whole_values_and_application_addresses();
    missing_target_failure_names_the_step_and_regions_and_stays_terminal();
    expectations_wait_for_changes_and_fail_at_their_deadlines();
    pending_queries_and_pauses_advance_at_most_one_step();
    unbound_disabled_and_noncomposer_controls_reject_scripted_acts();
}

#[test]
fn random_people_replay_cover_enabled_controls_and_reject_unavailable_nodes() {
    random_people_replay_the_same_seed_over_the_same_trees();
    a_sweep_presses_every_bound_node_and_reaches_fields_composers_and_history();
    random_people_never_act_on_unbound_disabled_or_unshown_nodes();
}

const LIMITS: Limits = Limits {
    nodes: 24,
    depth: 6,
    text_per_node: 80,
    text_bytes: 400,
    drawing_bytes: 32,
    styles_per_node: 0,
    patches: 144,
    bindings: 8,
    slots: 0,
    markdown_depth: 2,
};
fn build(fill: impl FnOnce(&mut Builder)) -> View {
    let mut view = View::new(&LIMITS);
    let mut builder = view.builder();
    fill(&mut builder);
    let mut patches = Queue::with_capacity(LIMITS.patches);
    view.finish(builder, 1, &mut patches);
    view
}
fn binding(n: u64) -> Binding {
    Binding { owner: Owner { number: 1 }, token: Token::new(n) }
}
fn region(name: &[u8]) -> Target {
    Target::Role { role: Role::Region, name: Box::from(name) }
}
fn fixture() -> View {
    build(|b| {
        b.open(Role::Page);
        b.open(Role::Region);
        b.name(b"Task T7");
        b.open(Role::Region);
        b.name(b"Proposal from I");
        b.open(Role::Button);
        b.name(b"Accept proposal");
        b.bind(binding(1));
        b.close();
        b.open(Role::Text);
        b.run(Run::Plain, b"Ready to ");
        b.run(Run::Strong, b"release");
        b.close();
        b.open(Role::TextArea);
        b.name(b"Reply");
        b.bind(binding(2));
        b.state(State::SubmitsOnEnter);
        b.value(b"", 0);
        b.close();
        b.open(Role::TextField);
        b.name(b"Subject");
        b.bind(binding(3));
        b.close();
        b.open(Role::Button);
        b.name(b"Disabled");
        b.bind(binding(4));
        b.state(State::Disabled);
        b.close();
        b.drawing(DrawingKind { owner: Owner { number: 1 }, kind: 1 }, b"Error rate", b"secret data", 32);
        b.text(Run::Plain, b"30 % since 10:02");
        b.close();
        b.close();
        b.close();
        b.open(Role::Region);
        b.name(b"Task T8");
        b.open(Role::Button);
        b.name(b"Release");
        b.close();
        b.text(Run::Plain, "CAFÉ".as_bytes());
        b.close();
        b.close();
    })
}

fn finds_nested_regions_and_does_not_escape_their_subtrees() {
    let view = fixture();
    let mut face = TreeFace::new(view.tree());
    let find = Find::named(Role::Button, b"ACCEPT").within(Box::from([region(b"t7"), region(b"from i")]));
    assert!(matches!(face.find(&find), Found::Node(_)));
    assert_eq!(face.find(&Find::named(Role::Button, b"Release").within(Box::from([region(b"T7")]))), Found::Absent);
    assert_eq!(
        face.find(&Find::named(Role::Button, b"Accept").within(Box::from([region(b"T8"), region(b"from I")]))),
        Found::Absent
    );
}

fn text_queries_join_runs_and_ignore_case() {
    let view = fixture();
    let mut face = TreeFace::new(view.tree());
    assert!(matches!(face.find(&Find::text(b"to RELEASE")), Found::Node(_)));
    assert!(matches!(face.find(&Find::text("café".as_bytes())), Found::Node(_)));
    assert_eq!(face.find(&Find::named(Role::Link, b"Accept")), Found::Absent);
    let unnamed = build(|b| {
        b.open(Role::Button);
        b.text(Run::Plain, b"Continue");
        b.close();
    });
    assert!(matches!(TreeFace::new(unnamed.tree()).find(&Find::named(Role::Button, b"continue")), Found::Node(_)));
}

fn drawings_are_found_by_name_and_fallback_never_by_data() {
    let view = fixture();
    let mut face = TreeFace::new(view.tree());
    assert!(matches!(face.find(&Find::named(Role::Drawing, b"error RATE")), Found::Node(_)));
    let fallback = Find::text(b"30 %")
        .within(Box::from([Target::Role { role: Role::Drawing, name: Box::from(b"Error".as_slice()) }]));
    assert!(matches!(face.find(&fallback), Found::Node(_)));
    assert_eq!(face.find(&Find::text(b"secret data")), Found::Absent);
}

fn a_script_reports_shell_acts_whole_values_and_application_addresses() {
    let view = fixture();
    let mut face = TreeFace::new(view.tree());
    let button = match face.find(&Find::named(Role::Button, b"Accept")) {
        Found::Node(id) => id,
        Found::Absent | Found::Pending => unreachable!(),
    };
    let field = match face.find(&Find::named(Role::TextArea, b"Reply")) {
        Found::Node(id) => id,
        Found::Absent | Found::Pending => unreachable!(),
    };
    let mut person = Person::new(Box::from([
        Step::Go { address: String::from("inbox") },
        Step::Press { find: Find::named(Role::Button, b"Accept") },
        Step::Type { find: Find::named(Role::TextArea, b"Reply"), words: Box::from(b"all my words".as_slice()) },
        Step::Send { find: Find::named(Role::TextArea, b"Reply") },
        Step::Reload,
    ]));
    assert_eq!(person.next(&mut face, Time::from_nanos(0)), Next::Do(Doing::Go { address: String::from("inbox") }));
    assert_eq!(person.next(&mut face, Time::from_nanos(0)), Next::Do(Doing::Act { node: button, act: Act::Press }));
    assert_eq!(
        person.next(&mut face, Time::from_nanos(0)),
        Next::Do(Doing::Act { node: field, act: Act::Input { value: Box::from(b"all my words".as_slice()) } })
    );
    assert_eq!(person.next(&mut face, Time::from_nanos(0)), Next::Do(Doing::Act { node: field, act: Act::Submit }));
    assert_eq!(person.next(&mut face, Time::from_nanos(0)), Next::Do(Doing::Reload));
    assert_eq!(person.next(&mut face, Time::from_nanos(0)), Next::Done);
    assert_eq!(person.at(), 5);
}

fn missing_target_failure_names_the_step_and_regions_and_stays_terminal() {
    let view = fixture();
    let mut face = TreeFace::new(view.tree());
    let find = Find::named(Role::Button, b"Release").within(Box::from([region(b"T7")]));
    let mut person = Person::<u8>::new(Box::from([Step::Reload, Step::Press { find: find.clone() }]));
    assert_eq!(person.next(&mut face, Time::from_nanos(0)), Next::Do(Doing::Reload));
    let failure = Next::Failed { step: 1, why: Why::Missing { find } };
    assert_eq!(person.next(&mut face, Time::from_nanos(0)), failure);
    assert_eq!(person.next(&mut face, Time::from_nanos(99)), failure);
    let Next::Failed { why, .. } = failure else { unreachable!() };
    assert_eq!(why.to_string(), "no button named Release in the region named T7");
}

fn expectations_wait_for_changes_and_fail_at_their_deadlines() {
    let view = fixture();
    let empty = build(|b| {
        b.open(Role::Page);
        b.close();
    });
    let mut face = TreeFace::new(view.tree());
    let mut absent = TreeFace::new(empty.tree());
    let find = Find::named(Role::Button, b"Accept");
    let mut see = Person::<u8>::new(Box::from([Step::See { find: find.clone(), within: Duration::from_nanos(10) }]));
    assert_eq!(see.next(&mut absent, Time::from_nanos(2)), Next::Wait { until: Some(Time::from_nanos(12)) });
    assert_eq!(see.next(&mut face, Time::from_nanos(12)), Next::Wait { until: None });
    assert_eq!(see.next(&mut face, Time::from_nanos(12)), Next::Done);
    let mut gone = Person::<u8>::new(Box::from([Step::Gone { find: find.clone(), within: Duration::from_nanos(10) }]));
    assert_eq!(gone.next(&mut face, Time::from_nanos(2)), Next::Wait { until: Some(Time::from_nanos(12)) });
    assert_eq!(gone.next(&mut absent, Time::from_nanos(3)), Next::Wait { until: None });
    let mut timeout =
        Person::<u8>::new(Box::from([Step::See { find: find.clone(), within: Duration::from_nanos(10) }]));
    assert_eq!(timeout.next(&mut absent, Time::from_nanos(2)), Next::Wait { until: Some(Time::from_nanos(12)) });
    assert_eq!(
        timeout.next(&mut absent, Time::from_nanos(12)),
        Next::Failed { step: 0, why: Why::Missing { find: find.clone() } }
    );
    let mut timeout =
        Person::<u8>::new(Box::from([Step::Gone { find: find.clone(), within: Duration::from_nanos(0) }]));
    assert_eq!(timeout.next(&mut face, Time::from_nanos(2)), Next::Failed { step: 0, why: Why::StillThere { find } });
}

fn pending_queries_and_pauses_advance_at_most_one_step() {
    struct Pending;
    impl Face for Pending {
        fn find(&mut self, _find: &Find) -> Found {
            Found::Pending
        }
    }
    let mut face = Pending;
    let mut person = Person::<u8>::new(Box::from([
        Step::Pause { span: Duration::from_nanos(10) },
        Step::Press { find: Find::named(Role::Button, b"Accept") },
    ]));
    assert_eq!(person.next(&mut face, Time::from_nanos(1)), Next::Wait { until: Some(Time::from_nanos(11)) });
    assert_eq!(person.next(&mut face, Time::from_nanos(10)), Next::Wait { until: Some(Time::from_nanos(11)) });
    assert_eq!(person.next(&mut face, Time::from_nanos(11)), Next::Wait { until: None });
    assert_eq!(person.at(), 1);
    assert_eq!(person.next(&mut face, Time::from_nanos(12)), Next::Wait { until: None });
    let find = Find::text(b"sent");
    let mut person = Person::<u8>::new(Box::from([Step::See { find: find.clone(), within: Duration::from_nanos(2) }]));
    assert_eq!(
        person.next(&mut face, Time::from_nanos(u64::MAX - 1)),
        Next::Wait { until: Some(Time::from_nanos(u64::MAX)) }
    );
    assert_eq!(
        person.next(&mut face, Time::from_nanos(u64::MAX)),
        Next::Failed { step: 0, why: Why::Missing { find } }
    );
}

fn unbound_disabled_and_noncomposer_controls_reject_scripted_acts() {
    let view = fixture();
    let mut face = TreeFace::new(view.tree());
    for step in [
        Step::Press { find: Find::named(Role::Button, b"Disabled") },
        Step::Press { find: Find::named(Role::Button, b"Release") },
        Step::Send { find: Find::named(Role::TextField, b"Subject") },
        Step::Type { find: Find::named(Role::Button, b"Accept"), words: Box::from(b"word".as_slice()) },
    ] {
        let mut person = Person::<u8>::new(Box::from([step]));
        assert!(matches!(
            person.next(&mut face, Time::from_nanos(0)),
            Next::Failed { why: Why::Unexpected { .. }, .. }
        ));
    }
}

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

fn a_sweep_presses_every_bound_node_and_reaches_fields_composers_and_history() {
    use std::collections::BTreeSet;
    let view = random_tree();
    let tree = view.tree();
    let expected: BTreeSet<_> = tree
        .nodes()
        .iter()
        .filter(|n| tree.bound(n.id).is_some() && !n.states.as_slice().contains(&State::Disabled))
        .map(|n| n.id)
        .collect();
    let mut pressed = BTreeSet::new();
    let mut inputs = BTreeSet::new();
    let mut submitted = BTreeSet::new();
    let mut toggled = BTreeSet::new();
    let mut alphabet = BTreeSet::new();
    let mut back = false;
    let mut reload = false;
    let mut empty = false;
    let mut full = false;
    for seed in 0..32 {
        let mut person = RandomPerson::new(seed, 32);
        for _ in 0..64 {
            match person.next(tree) {
                Some(RandomDoing::Act { node, act }) => {
                    let control = tree.find(node).expect("visible control");
                    assert!(TreeFace::new(tree).allows(node, &act));
                    match act {
                        Act::Press => {
                            pressed.insert(node);
                        }
                        Act::Input { value } => {
                            assert!(value.len() <= 32);
                            empty |= value.is_empty();
                            full |= value.len() == 32;
                            inputs.insert(node);
                            alphabet.extend(value.iter().copied());
                        }
                        Act::Submit => {
                            submitted.insert(node);
                            assert!(control.states.as_slice().contains(&State::SubmitsOnEnter));
                        }
                        Act::Drawing { .. } => panic!("the random person never makes drawing acts"),
                        Act::Toggle => {
                            toggled.insert(node);
                            assert_eq!(control.role, Role::Check);
                        }
                    }
                }
                Some(RandomDoing::Back) => back = true,
                Some(RandomDoing::Reload) => reload = true,
                None => unreachable!("tree has bound nodes"),
            }
        }
    }
    assert_eq!(pressed, expected);
    assert_eq!(inputs.len(), 2);
    assert_eq!(submitted.len(), 1);
    assert_eq!(toggled.len(), 1);
    assert!(back && reload && empty && full);
    for byte in b" 0123456789#*_`[]()>-+.!\n" {
        assert!(alphabet.contains(byte));
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

#[test]
fn a_subtree_face_distinguishes_repeated_regions_and_never_escapes() {
    let view = build(|b| {
        b.open(Role::Page);
        for word in [b"First".as_slice(), b"Second"] {
            b.open(Role::Region);
            b.name(b"Conversation");
            b.open(Role::Button);
            b.name(word);
            b.bind(binding(1));
            b.close();
            b.text(Run::Plain, word);
            b.close();
        }
        b.close();
    });
    let regions: Vec<_> = view.tree().nodes().iter().filter(|n| n.role == Role::Region).map(|n| n.id).collect();
    let mut face = TreeFace::subtree(view.tree(), regions[1]).expect("second region");
    assert!(matches!(face.find(&Find::named(Role::Button, b"second")), Found::Node(_)));
    assert!(matches!(face.find(&Find::text(b"SECOND")), Found::Node(_)));
    assert_eq!(face.find(&Find::named(Role::Button, b"First")), Found::Absent);
    let Found::Node(first) = TreeFace::new(view.tree()).find(&Find::named(Role::Button, b"First")) else {
        panic!("first button");
    };
    assert!(!face.allows(first, &Act::Press));
    assert!(TreeFace::subtree(view.tree(), NodeId(0)).is_none());
    assert!(TreeFace::subtree(view.tree(), NodeId(u32::MAX)).is_none());
    assert!(
        matches!(
            TreeFace::new(view.tree())
                .find(&Find::named(Role::Button, b"Second").within(Box::from([region(b"Conversation")]))),
            Found::Absent
        ),
        "whole-tree Find retains first-match behavior"
    );
}

#[test]
fn exact_names_distinguish_release_from_amend_and_release_without_escaping_subtrees_or_losing_fallbacks() {
    let view = build(|b| {
        b.open(Role::Page);
        b.open(Role::Region);
        b.name(b"Task");
        b.open(Role::Button);
        b.name(b"Amend and release");
        b.bind(binding(1));
        b.close();
        b.open(Role::Button);
        b.name(b"Release");
        b.bind(binding(2));
        b.close();
        b.close();
        b.open(Role::Region);
        b.name(b"Other");
        b.open(Role::Button);
        b.name(b"Release");
        b.bind(binding(3));
        b.close();
        b.open(Role::Button);
        b.bind(binding(4));
        b.text(Run::Plain, b"Con");
        b.open(Role::Text);
        b.run(Run::Strong, b"tinue");
        b.close();
        b.close();
        b.close();
        b.close();
    });
    let nodes = view.tree().nodes();
    let amend = nodes.iter().find(|n| n.name.as_deref() == Some(b"Amend and release")).expect("amend button").id;
    let release = nodes.iter().find(|n| n.name.as_deref() == Some(b"Release")).expect("release button").id;
    let other = nodes.iter().find(|n| n.name.as_deref() == Some(b"Other")).expect("other subtree").id;
    let mut face = TreeFace::new(view.tree());
    assert_eq!(face.find(&Find::named(Role::Button, b"release")), Found::Node(amend));
    let exact = Find::exactly_named(Role::Button, b"Release").within(Box::from([region(b"Task")]));
    assert_eq!(face.find(&exact), Found::Node(release));
    assert_eq!(face.find(&Find::exactly_named(Role::Button, b"release")), Found::Absent);
    assert_eq!(face.find(&Find::exactly_named(Role::Link, b"Release")), Found::Absent);
    assert_eq!(face.find(&Find::exactly_named(Role::Button, b"Amend")), Found::Absent);
    assert_eq!(
        face.find(&Find::exactly_named(Role::Button, b"Continue").within(Box::from([region(b"Task")]))),
        Found::Absent
    );
    let mut outside = TreeFace::subtree(view.tree(), other).expect("other subtree");
    let outside_release = outside.find(&Find::exactly_named(Role::Button, b"Release"));
    assert!(matches!(outside_release, Found::Node(id) if id != release));
    assert_ne!(outside.find(&exact), Found::Node(release));
    let fallback = outside.find(&Find::exactly_named(Role::Button, b"Continue"));
    assert!(matches!(fallback, Found::Node(_)));
    assert_eq!(outside.find(&Find::named(Role::Button, b"CONTINUE")), fallback);
    assert_eq!(outside.find(&Find::exactly_named(Role::Button, b"Con")), Found::Absent);
    assert_eq!(format!("{}", Find::exactly_named(Role::Button, b"Release")), "button named exactly Release");
}

#[test]
fn a_menu_item_inside_a_named_menu_is_found_and_pressable() {
    let view = build(|b| {
        b.open(Role::Page);
        b.open(Role::Menu);
        b.name(b"More");
        b.open(Role::MenuItem);
        b.name(b"Archive");
        b.bind(binding(1));
        b.close();
        b.close();
        b.close();
    });
    let mut face = TreeFace::new(view.tree());
    let menu = Target::Role { role: Role::Menu, name: Box::from(*b"More") };
    let Found::Node(item) = face.find(&Find::named(Role::MenuItem, b"Archive").within(Box::from([menu]))) else {
        panic!("the menu item is found inside its menu");
    };
    assert!(face.allows(item, &Act::Press));
}

#[test]
fn a_disclosures_content_is_found_by_its_text() {
    let view = build(|b| {
        b.open(Role::Page);
        b.open(Role::Disclosure);
        b.name(b"Details");
        b.text(Run::Plain, b"Waiting for an answer");
        b.close();
        b.close();
    });
    let mut face = TreeFace::new(view.tree());
    let disclosure = Target::Role { role: Role::Disclosure, name: Box::from(*b"Details") };
    let found = face.find(&Find::text(b"Waiting for an answer").within(Box::from([disclosure])));
    let Found::Node(node) = found else { panic!("the folded content remains readable") };
    assert_eq!(view.tree().find(node).unwrap().role, Role::Text);
}

#[test]
fn the_random_person_never_acts_on_a_landmark() {
    let view = build(|b| {
        b.open(Role::Page);
        for role in [
            Role::Banner,
            Role::Navigation,
            Role::Main,
            Role::Complementary,
            Role::ContentInfo,
            Role::Form,
            Role::Search,
        ] {
            b.open(role);
            b.open(Role::Button);
            b.bind(binding(1));
            b.close();
            b.close();
        }
        b.open(Role::Button);
        b.name(b"Continue");
        b.bind(binding(2));
        b.close();
        b.close();
    });
    let mut person = RandomPerson::new(7, 4);
    let mut acted = 0;
    let face = TreeFace::new(view.tree());
    for _ in 0..128 {
        match person.next(view.tree()) {
            Some(RandomDoing::Act { node, act }) => {
                assert_eq!(view.tree().find(node).unwrap().role, Role::Button);
                assert!(face.allows(node, &act));
                acted += 1;
            }
            Some(RandomDoing::Back | RandomDoing::Reload) | None => {}
        }
    }
    assert!(acted > 0);
}
