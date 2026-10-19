//! Scripted failure classes through the provider byte boundary.
use skein_fake_llm_domain::api::{self, Finish, Line, Script, Turn};
use skein_llm::{Block, client};
use skein_llm_world::{call, fake::Exchange, limits};

#[test]
fn a_line_naming_another_index_lets_the_call_through() {
    let mut input = call(1);
    input.prompt.instructions = b"caller-script".as_slice().into();
    let script = Script {
        cue: b"caller-script".as_slice().into(),
        turns: Box::new([Turn {
            lines: Box::new([
                Line::Fail {
                    error: api::Error::Exhausted { retry_after: Some(skein_lib::Duration::from_secs(1)) },
                    report: api::Report::none(),
                    credentials: Box::new([1]),
                },
                Line::Text { text: b"admitted".as_slice().into() },
            ]),
            finish: Finish::Stop,
            tokens: 1,
        }]),
    };
    let mut world = Exchange::new(input, limits(), Box::new([script]));
    world.start();
    world.run();
    assert_eq!(world.queries[0].credential, Some(0));
    let Some(completion) = world.seen.iter().find_map(|event| match event {
        client::Event::Completed { completion, .. } => Some(completion),
        client::Event::Delta { .. }
        | client::Event::Block { .. }
        | client::Event::Failed { .. }
        | client::Event::Cancelled { .. }
        | client::Event::Reusable
        | client::Event::Close
        | client::Event::Closed => None,
    }) else {
        panic!("{:?}", world.seen)
    };
    let [Block::Text { text, .. }] = completion.content.as_ref() else { panic!("{:?}", completion.content) };
    assert_eq!(text.as_ref(), b"admitted");
}
