//! A person reads notes by their link names through the real tree face.
use super::fixtures;
use crate::{Place, limits};
use skein_fake_person::{Find, Next, Person, Step, TreeFace};
use skein_lib::{Duration, Queue, Time};
use skein_ui::{Role, View};
#[test]
fn a_person_sees_each_note_on_the_list() {
    let state = fixtures::state();
    let limits = limits::limits();
    let mut view = View::new(&limits);
    state.build(&mut view, &mut Queue::with_capacity(limits.patches));
    let mut face = TreeFace::new(view.tree());
    let mut person = Person::<Place>::new(Box::from([
        Step::See { find: Find::exactly_named(Role::Link, fixtures::GROCERIES), within: Duration::ZERO },
        Step::See { find: Find::exactly_named(Role::Link, fixtures::DENTIST), within: Duration::ZERO },
    ]));
    for step in 0_u32..2 {
        assert_eq!(person.next(&mut face, Time::ZERO), Next::Wait { until: None }, "see step {step}");
    }
    assert_eq!(person.next(&mut face, Time::ZERO), Next::Done);
}
