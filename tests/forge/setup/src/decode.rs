//! Bounded tokenizer driver for reconcile's HTTP answers.
use skein_forge::forgejo::{
    client,
    decode::{Malformed, Step},
    document::{Document, Row},
    limits::Limits,
};
use skein_json::tokenizer;
use skein_lib::{
    Env, Intake, Queue, Time, Wall,
    stream::{Down, Up},
};
#[derive(Debug)]
pub struct Decoded {
    pub rows: Vec<Row>,
    pub document: Result<Document, Malformed>,
}
#[must_use]
pub fn decode_answer(mut decoder: client::Decoder, body: &[u8], limits: &Limits) -> Decoded {
    let mut rows = Vec::new();
    let mut document = None;
    let first = client::first(&decoder);
    drive(body, limits, first, |event| match client::step(&mut decoder, limits, event) {
        Step::Demand(next) => Some(next),
        Step::Row(row, next) => {
            rows.push(row);
            Some(next)
        }
        Step::Done(value) => {
            document = Some(Ok(value));
            None
        }
        Step::Malformed(error) => {
            document = Some(Err(error));
            None
        }
    });
    Decoded { rows, document: document.expect("terminal decoder") }
}
fn drive(
    body: &[u8],
    limits: &Limits,
    mut demand: tokenizer::Request,
    mut event: impl FnMut(&tokenizer::Event) -> Option<tokenizer::Request>,
) -> usize {
    // No decoded string can exceed the complete input, including malformed
    // mutations. Keep the configured bound while reserving only useful room.
    let mut json = limits.json;
    json.string = json.string.min(u32::try_from(body.len()).expect("body fits"));
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: json };
    let mut tokenizer = tokenizer::Tokenizer::new(&json);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut intake = Intake::with_capacity(
        u32::try_from(body.len()).expect("body fits").max(tokenizer::largest_demand(&limits.json)),
    );
    intake.append(body).expect("input fits");
    let mut read = None;
    for steps in 1..=body.len() * 4 + 32 {
        match read.take() {
            Some(read) => {
                let up = match intake.meet(read) {
                    Some(bytes) => Up::Bytes(bytes),
                    None => Up::End,
                };
                tokenizer::up(&mut tokenizer, &env, up, &mut above, &mut below);
            }
            None => tokenizer::down(&mut tokenizer, &env, demand, &mut above, &mut below),
        }
        match below.pop() {
            Some(Down::Demand { read: r, room: 0 }) => read = Some(r),
            None => {}
            Some(other) => panic!("unexpected demand {other:?}"),
        }
        if let Some(token) = above.pop() {
            match event(&token) {
                Some(next) => demand = next,
                None => return steps,
            }
        }
    }
    panic!("decoder exhausted its input step bound")
}
