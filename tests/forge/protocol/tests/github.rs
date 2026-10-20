use skein_forge::api::{Answer, Read, State};
use skein_forge_protocol_world::{World, fixtures};
use skein_lib::{Duration, Time};
use std::collections::BTreeSet;

fn invariants(world: &mut World) {
    world.finish();
    let (calls, answers) = world.calls_and_answers();
    assert!(calls > 0);
    assert_eq!(calls, answers);
    assert_eq!(world.owner_counts(), (0, 0));
    let a = world.accounting();
    assert_eq!(a.reported, a.written, "{a:?}");
    assert_eq!(a.served + a.cut_in_head, a.written, "{a:?}");
    assert!(world.secrets_absent());
    let (keys, jwts, tokens) = world.github_credentials();
    assert!(keys > 0 && jwts > 0 && tokens > 0);
}

#[test]
fn a_github_creation_with_a_lost_answer_is_found_after_restart() {
    let mut world = fixtures::github_lost_creation(23);
    assert_eq!(world.creations(), 1);
    assert!(!world.made(1));
    assert!(world.attempt(1).is_some());
    assert_eq!(world.released_writes(), 1);
    world.restart();
    invariants(&mut world);
    assert!(world.made(1));
    assert_eq!(world.creations(), 1);
    assert_eq!(world.discarded(), 1);
}

#[test]
fn github_listings_follow_another_partys_changes_across_pages() {
    let mut world = World::github(5, 0);
    world.run_for(Duration::from_secs(1));
    let a = world.person_issue(b"A");
    let b = world.person_issue(b"B");
    let c = world.person_issue(b"C");
    world.person_edit(b, b"B2");
    world.person_close(c);
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    let mut page = 1;
    loop {
        let Answer::Items { items, more, .. } =
            world.read(Read::Items { since: Time::ZERO, page, kind: None }).expect("listing")
        else {
            panic!("items")
        };
        for item in items {
            assert!(seen.insert(item.number));
            rows.push((item.number, item.title, item.state == State::Open));
        }
        if !more {
            break;
        }
        page += 1;
    }
    assert!(page > 1);
    rows.sort_by_key(|row| row.0);
    assert_eq!(rows, world.snapshot());
    assert_eq!(
        rows,
        vec![
            (a, Box::from(b"A".as_slice()), true),
            (b, Box::from(b"B2".as_slice()), true),
            (c, Box::from(b"C".as_slice()), false)
        ]
    );
    fixtures::github_close(&mut world, a);
    invariants(&mut world);
}
