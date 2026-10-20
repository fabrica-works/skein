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
        let played = Played::new(&CONFIG, scripts, &empty_menu()).expect("admitted scripts");
        let mut query = query(messages);
        query.system = copy_of(system);
        assert_eq!(played.choose(&query), chosen, "{label}");
        let reply = respond(&mut Rng::new(1), &mut 0, &CONFIG, &played, &empty_menu(), &query).expect(label).0;
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
        let played = Played::new(&CONFIG, scripts, &empty_menu()).expect("admitted scripts");
        let mut minted = 0;
        for ((prefix, turn), answer) in PREFIXES.into_iter().zip(expected_turns).zip(expected_answers) {
            let mut query = query(history.get(..prefix).expect("conversation prefix").into());
            query.system = copy_of(system);
            assert_eq!(played.choose(&query), Some((0, turn)), "{label} prefix {prefix}");
            let reply = respond(&mut Rng::new(1), &mut minted, &CONFIG, &played, &empty_menu(), &query).expect(label).0;
            assert_eq!(reply.parts.as_ref(), &[answer], "{label} prefix {prefix}");
        }
    }
}
