use super::*;

#[test]
fn head_and_base_moves_draw_replayable_inclusive_settle_deadlines() {
    for (min, max) in [(0, 0), (2, 2), (3, 7)] {
        for seed in [1, 7, 31] {
            let mut draws = List::with_capacity(2);
            for _replay in 0_u8..2 {
                let config = Config {
                    latency_min: Duration::ZERO,
                    latency_max: Duration::ZERO,
                    settle_min: Duration::from_secs(min),
                    settle_max: Duration::from_secs(max),
                    resolution: Duration::ZERO,
                    ..CALM
                };
                let mut h = Harness::seeded(config, setup(), seed);
                let work = h.commit(FIRST, &[(b"src", b"one")]);
                h.push(ENGINE, b"work", work).unwrap();
                h.open(b"work").unwrap();
                let mut deadlines = List::with_capacity(2);
                for (branch, parent, files) in [
                    (b"work".as_slice(), work, [(b"src".as_slice(), b"two".as_slice())]),
                    (MAIN, FIRST, [(b"other".as_slice(), b"base".as_slice())]),
                ] {
                    let next = h.commit(parent, &files);
                    let at = h.env.now;
                    h.push(ENGINE, branch, next).unwrap();
                    let repository = h.domain.repositories.get(h.domain.id(REPOSITORY)).unwrap();
                    let deadline = repository.items.get(&1).unwrap().pull.as_ref().unwrap().checked;
                    assert!(deadline >= at.saturating_add(config.settle_min));
                    assert!(deadline <= at.saturating_add(config.settle_max));
                    deadlines.push(deadline).expect("two branch moves");
                    if deadline > at {
                        h.env.now = Time::from_nanos(deadline.as_nanos().checked_sub(1).unwrap());
                        let Answer::Pull(pull) = h.ok(PERSON, read(Read::Pull { number: 1 })) else { panic!("pull") };
                        assert!(pull.checking);
                        assert!(!pull.mergeable);
                    }
                    h.env.now = deadline;
                    let Answer::Pull(pull) = h.ok(PERSON, read(Read::Pull { number: 1 })) else { panic!("pull") };
                    assert!(!pull.checking);
                    assert!(pull.mergeable);
                }
                draws.push(deadlines).expect("two replays");
            }
            assert_eq!(draws.get(0), draws.get(1), "range {min}..={max}, seed {seed}");
        }
    }
}

#[test]
#[should_panic(expected = "latencies are ranges")]
fn reversed_settle_range_is_refused_at_admission() {
    let _domain =
        Domain::new(&Config { settle_min: Duration::from_secs(3), settle_max: Duration::from_secs(2), ..CALM }, 7);
}
