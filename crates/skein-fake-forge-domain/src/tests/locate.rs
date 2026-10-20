//! A repository located with read permission (protocol/forge.md, 12).
use super::*;

#[test]
fn a_locate_answers_the_id_the_scenario_gave_the_repository() {
    let mut repository = setup();
    repository.id = 7;
    let mut h = Harness::with(CALM, repository);
    assert_eq!(h.call(PERSON, read(Read::Locate)), Ok(Answer::Located { id: 7 }));
}

#[test]
fn a_locate_by_a_user_who_may_not_read_is_forbidden() {
    let mut h = Harness::new(CALM);
    assert_eq!(h.call(99, read(Read::Locate)), Err(Error::Forbidden));
}
