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

mod random;

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
    scene_marks: 16,
    scene_bytes: 256,
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
const CONTROL: NodeId = NodeId(2); // page, control
fn page_with_control(role: Role, name: &[u8]) -> View {
    build(|b| {
        b.open(Role::Page);
        b.open(role);
        b.name(name);
        b.bind(binding(1));
        b.close();
        b.close();
    })
}

#[test]
fn a_press_on_a_check_is_a_toggle() {
    let view = page_with_control(Role::Check, b"Notify me");
    let mut person: Person<u8> = Person::new(Box::new([Step::Press { find: Find::named(Role::Check, b"Notify") }]));
    let next = person.next(&mut TreeFace::new(view.tree()), Time::ZERO);
    assert_eq!(next, Next::Do(Doing::Act { node: CONTROL, act: Act::Toggle }));
}

#[test]
fn a_press_on_a_choice_is_a_toggle() {
    let view = page_with_control(Role::Choice, b"Daily");
    let mut person: Person<u8> = Person::new(Box::new([Step::Press { find: Find::named(Role::Choice, b"Daily") }]));
    let next = person.next(&mut TreeFace::new(view.tree()), Time::ZERO);
    assert_eq!(next, Next::Do(Doing::Act { node: CONTROL, act: Act::Toggle }));
}

#[test]
fn a_press_on_a_button_stays_a_press() {
    let view = page_with_control(Role::Button, b"Save");
    let mut person: Person<u8> = Person::new(Box::new([Step::Press { find: Find::named(Role::Button, b"Save") }]));
    let next = person.next(&mut TreeFace::new(view.tree()), Time::ZERO);
    assert_eq!(next, Next::Do(Doing::Act { node: CONTROL, act: Act::Press }));
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

const UNSHOWN: NodeId = NodeId(0); // no visible preorder position
const ABSENT: NodeId = NodeId(u32::MAX); // outside the fixture preorder
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
    assert!(TreeFace::subtree(view.tree(), UNSHOWN).is_none());
    assert!(TreeFace::subtree(view.tree(), ABSENT).is_none());
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
