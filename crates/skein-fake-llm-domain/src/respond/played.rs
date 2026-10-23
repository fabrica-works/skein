//! Request selection and turn counting (fake-llm.md, 2.4).
use super::respond;
use crate::api::{Finish, Line, Message, Part, Scripts, Turn};
use crate::domain::Played;
use crate::tests::{CONFIG, empty_menu, query, says, script, user};
use alloc::boxed::Box;
use skein_lib::Rng;
use skein_lib::bytes::copy_of;

fn text(text: &[u8]) -> Part {
    Part::Text { text: copy_of(text) }
}
fn request(parts: Box<[Part]>) -> Message {
    user(parts)
}
fn assistant(parts: Box<[Part]>) -> Message {
    Message { role: crate::api::Role::Assistant, parts }
}
fn scripts() -> Scripts {
    Scripts {
        cued: Box::new([script(b"system", Box::new([says(b"cued")]))]),
        asked: Box::new([
            script(b"ask", Box::new([says(b"asked")])),
            script(b"ask", Box::new([says(b"tie")])),
            script(b"first", Box::new([says(b"earlier")])),
        ]),
        otherwise: Box::new([says(b"otherwise")]),
    }
}

struct Choice {
    label: &'static str,
    system: &'static [u8],
    messages: Box<[Message]>,
    scripts: Scripts,
    chosen: Option<(usize, usize)>,
    answer: &'static [u8],
}

#[test]
#[expect(clippy::too_many_lines, reason = "one selection table keeps its fifteen input and answer rows together")]
fn system_request_and_fallback_choose_in_order() {
    let latest = || Box::<[Message]>::from([request(Box::new([text(b"ask")]))]);
    let no_fallback = Scripts { otherwise: Box::new([]), ..scripts() };
    let cued_only = Scripts { asked: Box::new([]), otherwise: Box::new([]), ..scripts() };
    let rows = [
        Choice {
            label: "system beats request",
            system: b"system",
            messages: latest(),
            scripts: scripts(),
            chosen: Some((0, 0)),
            answer: b"cued",
        },
        Choice {
            label: "earlier request position",
            system: b"",
            messages: Box::new([request(Box::new([text(b"first ask")]))]),
            scripts: scripts(),
            chosen: Some((3, 0)),
            answer: b"earlier",
        },
        Choice {
            label: "first listed at same position",
            system: b"",
            messages: latest(),
            scripts: scripts(),
            chosen: Some((1, 0)),
            answer: b"asked",
        },
        Choice {
            label: "earlier user cue ignored",
            system: b"",
            messages: Box::new([
                request(Box::new([text(b"ask")])),
                assistant(Box::new([text(b"reply")])),
                request(Box::new([text(b"new")])),
            ]),
            scripts: no_fallback.clone(),
            chosen: None,
            answer: b"done",
        },
        Choice {
            label: "second text part",
            system: b"",
            messages: Box::new([request(Box::new([text(b"new"), text(b"ask")]))]),
            scripts: scripts(),
            chosen: Some((1, 0)),
            answer: b"asked",
        },
        Choice {
            label: "parts ordered before cue positions",
            system: b"",
            messages: Box::new([request(Box::new([text(b"..first"), text(b"ask")]))]),
            scripts: scripts(),
            chosen: Some((3, 0)),
            answer: b"earlier",
        },
        Choice {
            label: "split cue never joined",
            system: b"",
            messages: Box::new([request(Box::new([text(b"as"), text(b"k")]))]),
            scripts: no_fallback.clone(),
            chosen: None,
            answer: b"done",
        },
        Choice {
            label: "neither cue uses fallback",
            system: b"unmatched",
            messages: Box::new([request(Box::new([text(b"new")]))]),
            scripts: scripts(),
            chosen: Some((4, 0)),
            answer: b"otherwise",
        },
        Choice {
            label: "empty fallback uses random",
            system: b"",
            messages: Box::new([request(Box::new([text(b"new")]))]),
            scripts: no_fallback,
            chosen: None,
            answer: b"done",
        },
        Choice {
            label: "only cued matches",
            system: b"system",
            messages: latest(),
            scripts: cued_only.clone(),
            chosen: Some((0, 0)),
            answer: b"cued",
        },
        Choice {
            label: "only cued misses",
            system: b"",
            messages: latest(),
            scripts: cued_only,
            chosen: None,
            answer: b"done",
        },
        Choice {
            label: "empty text is latest request",
            system: b"",
            messages: Box::new([request(Box::new([text(b"ask")])), request(Box::new([text(b"")]))]),
            scripts: scripts(),
            chosen: Some((4, 0)),
            answer: b"otherwise",
        },
        Choice {
            label: "no user text counts every assistant",
            system: b"",
            messages: Box::new([
                assistant(Box::new([text(b"reply")])),
                request(Box::new([Part::Opaque { bytes: copy_of(b"opaque") }])),
            ]),
            scripts: scripts(),
            chosen: Some((4, 1)),
            answer: b"done",
        },
        Choice {
            label: "no text no assistant starts fallback",
            system: b"",
            messages: Box::new([request(Box::new([]))]),
            scripts: scripts(),
            chosen: Some((4, 0)),
            answer: b"otherwise",
        },
        Choice {
            label: "empty request cue",
            system: b"",
            messages: Box::new([request(Box::new([text(b"")]))]),
            scripts: Scripts { asked: Box::new([script(b"", Box::new([says(b"empty")]))]), ..scripts() },
            chosen: Some((1, 0)),
            answer: b"empty",
        },
    ];
    for Choice { label, system, messages, scripts, chosen, answer } in rows {
        let mut played = Played::new(&CONFIG, scripts, &empty_menu()).expect("admitted scripts");
        let mut query = query(messages);
        query.system = copy_of(system);
        assert_eq!(played.choose(&query), chosen, "{label}");
        let reply =
            respond(&mut Rng::new(1), &mut 0, &CONFIG, &mut played, &empty_menu(), &query, skein_lib::Time::ZERO)
                .expect(label)
                .0;
        assert_eq!(&*reply.parts, &[text(answer)], "{label}");
    }
}

fn tool_turn() -> Turn {
    Turn {
        lines: Box::new([Line::Call { name: copy_of(b"ls"), arguments: copy_of(b"{}") }]),
        finish: Finish::ToolCalls,
        tokens: 2,
    }
}

fn tool_call(id: &[u8]) -> Part {
    Part::ToolCall { id: copy_of(id), name: copy_of(b"ls"), arguments: copy_of(b"{}") }
}

#[test]
fn request_turns_restart_while_system_turns_keep_counting() {
    const ROUND_CALL: &[u8] = b"call_0000000000000001";
    const NEXT_CALL: &[u8] = b"call_0000000000000002";
    // The request, tool round, continuation past the last turn, then a new request.
    const PREFIXES: [usize; 4] = [1, 3, 5, 7];
    let history = [
        request(Box::new([text(b"ask")])),
        assistant(Box::new([tool_call(ROUND_CALL)])),
        request(Box::new([Part::ToolOutput { id: copy_of(ROUND_CALL), output: copy_of(b"ok"), is_error: false }])),
        assistant(Box::new([text(b"answer")])),
        request(Box::new([Part::Opaque { bytes: copy_of(b"continuation") }])),
        assistant(Box::new([text(b"done")])),
        request(Box::new([text(b"ask again")])),
    ];
    let turns = || Box::<[Turn]>::from([tool_turn(), says(b"answer")]);
    let request_answers = [tool_call(ROUND_CALL), text(b"answer"), text(b"done"), tool_call(NEXT_CALL)];
    let system_answers = [tool_call(ROUND_CALL), text(b"answer"), text(b"done"), text(b"done")];
    let rows = [
        (
            "asked",
            Scripts { cued: Box::new([]), asked: Box::new([script(b"ask", turns())]), otherwise: Box::new([]) },
            b"".as_slice(),
            [0, 1, 2, 0],
            request_answers.clone(),
        ),
        (
            "otherwise",
            Scripts { cued: Box::new([]), asked: Box::new([]), otherwise: turns() },
            b"".as_slice(),
            [0, 1, 2, 0],
            request_answers,
        ),
        (
            "system",
            Scripts { cued: Box::new([script(b"system", turns())]), asked: Box::new([]), otherwise: Box::new([]) },
            b"system".as_slice(),
            [0, 1, 2, 3],
            system_answers,
        ),
    ];
    for (label, scripts, system, expected_turns, expected_answers) in rows {
        let mut played = Played::new(&CONFIG, scripts, &empty_menu()).expect("admitted scripts");
        let mut minted = 0;
        for ((prefix, turn), answer) in PREFIXES.into_iter().zip(expected_turns).zip(expected_answers) {
            let mut query = query(history.get(..prefix).expect("conversation prefix").into());
            query.system = copy_of(system);
            assert_eq!(played.choose(&query), Some((0, turn)), "{label} prefix {prefix}");
            let reply = respond(
                &mut Rng::new(1),
                &mut minted,
                &CONFIG,
                &mut played,
                &empty_menu(),
                &query,
                skein_lib::Time::ZERO,
            )
            .expect(label)
            .0;
            assert_eq!(reply.parts.as_ref(), &[answer], "{label} prefix {prefix}");
        }
    }
}

fn input_rules() -> crate::api::Rules {
    use crate::api::{Rule, Rules};
    Rules {
        cued: Box::new([Box::new([Rule { cue: copy_of(b"expected"), turn: 1 }, Rule { cue: copy_of(b""), turn: 0 }])]),
        asked: Box::new([]),
    }
}

fn input_scripts() -> Scripts {
    Scripts {
        cued: Box::new([script(b"system", Box::new([says(b"wait"), says(b"answer")]))]),
        asked: Box::new([]),
        otherwise: Box::new([]),
    }
}

fn input_query(messages: Box<[Message]>) -> crate::api::Query {
    let mut query = query(messages);
    query.system = copy_of(b"system expected");
    query
}

fn assert_input(messages: Box<[Message]>, turn: usize, answer: &[u8]) {
    let rules = input_rules();
    let mut played = Played::by_input(&CONFIG, input_scripts(), &empty_menu(), &rules).expect("input scripts");
    let query = input_query(messages);
    assert_eq!(played.choose_by_input(&query, &rules), Some((0, turn)));
    let reply = super::by_input(
        &mut Rng::new(1),
        &mut 0,
        &CONFIG,
        &mut played,
        &rules,
        &empty_menu(),
        &query,
        skein_lib::Time::ZERO,
    )
    .expect("input-selected answer")
    .0;
    assert_eq!(&*reply.parts, &[text(answer)]);
    assert_eq!(reply.finish, Finish::Stop);
    assert_eq!(reply.usage.output, Some(2));
}

#[test]
fn fresh_news_alone_answers_wait() {
    assert_input(
        Box::new([assistant(Box::new([text(b"wait")])), request(Box::new([text(b"forge landing news")]))]),
        0,
        b"wait",
    );
}

#[test]
fn fresh_expected_input_alone_answers_its_turn() {
    assert_input(
        Box::new([assistant(Box::new([text(b"wait")])), request(Box::new([text(b"expected")]))]),
        1,
        b"answer",
    );
}

#[test]
fn fresh_news_and_expected_input_answer_expected_turn() {
    assert_input(
        Box::new([
            assistant(Box::new([text(b"wait")])),
            request(Box::new([text(b"news")])),
            request(Box::new([text(b"expected")])),
        ]),
        1,
        b"answer",
    );
}

#[test]
fn older_expected_input_with_only_fresh_news_answers_wait() {
    assert_input(
        Box::new([
            request(Box::new([text(b"expected")])),
            assistant(Box::new([text(b"expected answer")])),
            request(Box::new([text(b"news")])),
        ]),
        0,
        b"wait",
    );
}

#[test]
fn fresh_tool_output_and_text_select_the_same_turn() {
    for (output, words) in [(b"expected".as_slice(), b"news".as_slice()), (b"news", b"expected")] {
        assert_input(
            Box::new([
                assistant(Box::new([tool_call(b"waiting")])),
                request(Box::new([
                    Part::ToolOutput { id: copy_of(b"waiting"), output: copy_of(output), is_error: false },
                    text(words),
                ])),
            ]),
            1,
            b"answer",
        );
    }
}

#[test]
fn a_repeated_input_plays_the_same_turn_without_extra_random_draws() {
    let rules = input_rules();
    let mut played = Played::by_input(&CONFIG, input_scripts(), &empty_menu(), &rules).expect("input scripts");
    let mut counted = Played::new(&CONFIG, input_scripts(), &empty_menu()).expect("counted scripts");
    let query = input_query(Box::new([assistant(Box::new([text(b"wait")])), request(Box::new([text(b"expected")]))]));
    let repeated = input_query(Box::new([
        assistant(Box::new([text(b"wait")])),
        request(Box::new([text(b"expected")])),
        assistant(Box::new([text(b"answer")])),
        request(Box::new([text(b"expected")])),
    ]));
    let mut input_rng = Rng::new(1);
    let mut counted_rng = Rng::new(1);
    for input_query in [&query, &query, &repeated] {
        let input = super::by_input(
            &mut input_rng,
            &mut 0,
            &CONFIG,
            &mut played,
            &rules,
            &empty_menu(),
            input_query,
            skein_lib::Time::ZERO,
        );
        let counted =
            respond(&mut counted_rng, &mut 0, &CONFIG, &mut counted, &empty_menu(), &query, skein_lib::Time::ZERO);
        assert_eq!(input, counted);
        assert_eq!(&*input.expect("same answer on every request").0.parts, &[text(b"answer")]);
        assert_eq!(input_rng.next_u64(), counted_rng.next_u64());
    }
}

#[test]
fn the_first_request_searches_the_whole_conversation() {
    assert_input(Box::new([request(Box::new([text(b"expected")])), request(Box::new([text(b"later")]))]), 1, b"answer");
}

#[test]
fn unruled_scripts_beside_ruled_scripts_keep_counting() {
    use crate::api::Rules;
    let mut scripts = input_scripts();
    scripts.cued = Box::new([
        scripts.cued.first().expect("ruled script").clone(),
        script(b"empty", Box::new([says(b"zero"), says(b"one")])),
        script(b"missing", Box::new([says(b"zero"), says(b"one")])),
    ]);
    let rules = Rules {
        cued: Box::new([input_rules().cued.first().expect("rules").clone(), Box::new([])]),
        asked: Box::new([]),
    };
    let mut played = Played::by_input(&CONFIG, scripts, &empty_menu(), &rules).expect("mixed scripts");
    for (system, script, turn, expected) in
        [(b"system".as_slice(), 0, 0, b"wait".as_slice()), (b"empty", 1, 1, b"one"), (b"missing", 2, 1, b"one")]
    {
        let mut query =
            input_query(Box::new([assistant(Box::new([text(b"zero")])), request(Box::new([text(b"news")]))]));
        query.system = copy_of(system);
        assert_eq!(played.choose_by_input(&query, &rules), Some((script, turn)));
        let reply = super::by_input(
            &mut Rng::new(1),
            &mut 0,
            &CONFIG,
            &mut played,
            &rules,
            &empty_menu(),
            &query,
            skein_lib::Time::ZERO,
        )
        .expect("mixed script answer")
        .0;
        assert_eq!(&*reply.parts, &[text(expected)]);
    }
}

#[test]
fn input_cues_ignore_system_opaque_and_split_parts() {
    assert_input(
        Box::new([
            assistant(Box::new([text(b"expected")])),
            request(Box::new([Part::Opaque { bytes: copy_of(b"expected") }, text(b"expec"), text(b"ted")])),
        ]),
        0,
        b"wait",
    );
}

#[test]
fn asked_rules_use_their_group_index_after_script_selection() {
    use crate::api::Rules;
    let mut scripts = input_scripts();
    scripts.asked = Box::new([script(b"ask", Box::new([says(b"wait"), says(b"answer")]))]);
    scripts.otherwise = Box::new([says(b"fallback")]);
    let rules = Rules { cued: Box::new([]), asked: input_rules().cued };
    let mut played = Played::by_input(&CONFIG, scripts, &empty_menu(), &rules).expect("asked rules");
    let mut query =
        query(Box::new([assistant(Box::new([text(b"previous")])), request(Box::new([text(b"ask expected")]))]));
    assert_eq!(played.choose_by_input(&query, &rules), Some((1, 1)));
    let reply = super::by_input(
        &mut Rng::new(1),
        &mut 0,
        &CONFIG,
        &mut played,
        &rules,
        &empty_menu(),
        &query,
        skein_lib::Time::ZERO,
    )
    .expect("asked input answer")
    .0;
    assert_eq!(&*reply.parts, &[text(b"answer")]);
    query.system = copy_of(b"system");
    assert_eq!(played.choose_by_input(&query, &rules), Some((0, 1)), "system script still wins");
    query.system = copy_of(b"");
    query.messages = Box::new([request(Box::new([text(b"expected")]))]);
    assert_eq!(played.choose_by_input(&query, &rules), Some((2, 0)), "rules never select a script");
}

#[test]
fn rule_order_wins_over_part_and_message_order() {
    use crate::api::Rule;
    let rules = crate::api::Rules {
        cued: Box::new([Box::new([
            Rule { cue: copy_of(b"expected"), turn: 1 },
            Rule { cue: copy_of(b"news"), turn: 0 },
            Rule { cue: copy_of(b""), turn: 0 },
        ])]),
        asked: Box::new([]),
    };
    let played = Played::by_input(&CONFIG, input_scripts(), &empty_menu(), &rules).expect("ordered rules");
    for messages in [
        Box::<[Message]>::from([request(Box::new([text(b"news"), text(b"expected")]))]),
        Box::new([request(Box::new([text(b"news")])), request(Box::new([text(b"expected")]))]),
    ] {
        assert_eq!(played.choose_by_input(&input_query(messages), &rules), Some((0, 1)));
    }
    assert_eq!(played.choose_by_input(&input_query(Box::new([])), &rules), Some((0, 0)), "empty cue always matches");
}
