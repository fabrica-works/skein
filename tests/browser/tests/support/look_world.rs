#![expect(clippy::disallowed_types, reason = "the kit's own tests of the look's reads (browser.md, 5.1)")]
use skein_browser::boundary::{Event, Look, Looked, Motion, Refusal, Request, Scheme};
use skein_browser_world::{
    referee::Referee,
    world::{Fault, World},
};
use skein_lib::{List, Time, Token};
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
pub fn run(cases: u64) {
    let mut covered = [false; 8];
    for seed in 0..cases {
        let mut draw = seed + 0x7f42_9abc;
        let mut world = World::new();
        world.start();
        world.open();
        world.sent.clear();
        let mut referee = Referee::new();
        let kind = usize::try_from(next(&mut draw) % 8).unwrap();
        covered[kind] = true;
        let look = match kind {
            0 => Look::Viewport { width: 390, height: 844, mobile: true, scheme: Scheme::Dark, motion: Motion::Reduce },
            1 | 7 => Look::Tree,
            2 => {
                world.selected = if next(&mut draw) & 1 == 0 { 7 } else { 0 };
                Look::Select { selector: b"#part".as_slice().into() }
            }
            3 | 4 => Look::Style { node: 7, properties: List::with_capacity(0) },
            _ => Look::Shot { clip: None, scale: 2 },
        };
        if kind == 4 {
            world.fault(b"CSS.getPlatformFontsForNode", Fault::Vanish);
        }
        if kind == 6 {
            world.env.limits.screenshot = 7;
        }
        let fault = match next(&mut draw) % 5 {
            0 => Fault::Drop,
            1 => Fault::Delay,
            2 => Fault::Crash,
            3 => Fault::PageCrash,
            _ => Fault::Interleave,
        };
        if kind == 7 {
            world.fault(b"Accessibility.getFullAXTree", fault);
        }
        referee.look_op(Token::new(30), world.env.limits);
        world.ask(Request::Look { page: Token::new(3), op: Token::new(30), look });
        if kind == 7 && (fault == Fault::Drop || fault == Fault::Delay) {
            if fault == Fault::Delay && next(&mut draw) & 1 == 0 {
                world.release_delayed();
            } else {
                world.tick(Time::from_nanos(world.env.limits.answer.as_nanos()));
                world.release_delayed();
            }
        }
        for bytes in &world.sent {
            referee.command(bytes);
        }
        let mut terminals = 0;
        while let Some(event) = world.above.pop() {
            referee.event(&event, 0);
            if let Event::Looked { looked: Looked::Tree(entries), .. } = &event {
                let entry = entries.iter().find(|entry| entry.name.as_ref() == b"Title").expect("tree field");
                assert_eq!(entry.value.as_ref(), b"Words");
                assert_eq!(
                    entry.states,
                    skein_browser::boundary::States {
                        focused: true,
                        disabled: true,
                        checked: true,
                        expanded: true,
                        selected: true
                    }
                );
                assert!(entry.multiline);
            }
            if matches!(event, Event::Looked { .. } | Event::Refused { .. }) {
                terminals += 1;
                if kind == 4 {
                    assert!(matches!(event, Event::Refused { why: Refusal::Gone, .. }), "seed {seed}: {event:?}");
                }
                if kind == 6 {
                    assert!(matches!(event, Event::Refused { why: Refusal::Limit, .. }), "seed {seed}: {event:?}");
                }
            }
        }
        assert_eq!(terminals, 1, "seed {seed}");
        world.assert_commands_settled();
    }
    assert!(covered.into_iter().all(|seen| seen), "look seeds reach each scenario");
}
