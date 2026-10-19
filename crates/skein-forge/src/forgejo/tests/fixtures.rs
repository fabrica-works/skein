use crate::forgejo::{
    client,
    decode::{Malformed, Step},
    document::{Document, Row},
    limits::Limits,
};
use skein_json::tokenizer;
use skein_lib::{
    Env, Intake, List, Queue, Time, Wall,
    stream::{Down, Up},
};
pub fn bot() -> crate::forgejo::document::User {
    crate::forgejo::document::User {
        id: 1,
        login: Box::from(*b"fixture"),
        is_admin: true,
        full_name: Box::from(*b"kit:bot"),
    }
}
pub fn user(id: u64, login: &[u8]) -> crate::forgejo::document::User {
    crate::forgejo::document::User { id, login: Box::from(login), is_admin: false, full_name: Box::from([]) }
}
pub fn limits() -> Limits {
    Limits {
        json: tokenizer::Limits { depth: 16, string: 65_536, number: 32, chunk: 4_096, length: 4_194_304 },
        writer: skein_json::writer::Limits { depth: 16, length: 4_194_304 },
        string_bytes: 1024,
        body_bytes: 65_536,
        inner: 64,
        target_bytes: 4_096,
    }
}
pub fn small() -> Limits {
    Limits { string_bytes: 8, body_bytes: 16, ..limits() }
}
pub fn capped(body: &[u8], cap: u32, limits: &Limits) -> tokenizer::Event {
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.json };
    let mut tokenizer = tokenizer::Tokenizer::new(&limits.json);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut intake =
        Intake::with_capacity(u32::try_from(body.len()).unwrap().max(tokenizer::largest_demand(&limits.json)));
    intake.append(body).unwrap();
    tokenizer::down(&mut tokenizer, &env, tokenizer::Request::Text(cap), &mut above, &mut below);
    for _ in 0..body.len().saturating_mul(4).saturating_add(32) {
        if let Some(event) = above.pop() {
            return event;
        }
        match below.pop().expect("tokenizer demand") {
            Down::Demand { read, room: 0 } => {
                let up = match intake.meet(read) {
                    Some(bytes) => Up::Bytes(bytes),
                    None => Up::End,
                };
                tokenizer::up(&mut tokenizer, &env, up, &mut above, &mut below);
            }
            other @ (Down::Demand { .. } | Down::Send(_) | Down::Finish) => panic!("unexpected demand {other:?}"),
        }
    }
    panic!("capped text exhausted the input step bound")
}
pub struct Decoded {
    pub rows: List<Row>,
    pub document: Result<Document, Malformed>,
}
pub fn decode(decoder: client::Decoder, body: &[u8], limits: &Limits) -> Decoded {
    drive(decoder, body, limits, None)
}
pub fn decode_cut(decoder: client::Decoder, body: &[u8], limits: &Limits, after_rows: u32) -> Decoded {
    drive(decoder, body, limits, Some(after_rows))
}
fn drive(mut decoder: client::Decoder, body: &[u8], limits: &Limits, cut_at: Option<u32>) -> Decoded {
    let mut pump = Pump::new(body, limits);
    let mut rows = List::with_capacity(64);
    let mut demand = client::first(&decoder);
    loop {
        let event = pump.next(demand);
        match client::step(&mut decoder, limits, &event) {
            Step::Demand(next) => demand = next,
            Step::Row(row, next) => {
                rows.push(row).expect("test rows");
                demand = if cut_at == Some(rows.len()) { client::cut(&mut decoder) } else { next };
            }
            Step::Done(value) => return Decoded { rows, document: Ok(value) },
            Step::Malformed(error) => return Decoded { rows, document: Err(error) },
        }
    }
}
pub fn deliver(header: &[u8], body: &[u8], limits: &Limits) -> Result<crate::forgejo::delivery::Delivery, Malformed> {
    use crate::forgejo::client::delivery::{self, Step};
    let mut decoder = delivery::decoder(header, limits).expect("known delivery");
    let mut pump = Pump::new(body, limits);
    let mut demand = delivery::first(&decoder);
    loop {
        let event = pump.next(demand);
        match delivery::step(&mut decoder, limits, &event) {
            Step::Demand(next) => demand = next,
            Step::Done(value) => return Ok(value),
            Step::Malformed(error) => return Err(error),
        }
    }
}
struct Pump {
    env: Env<tokenizer::Limits>,
    tokenizer: tokenizer::Tokenizer,
    above: Queue<tokenizer::Event>,
    below: Queue<Down>,
    intake: Intake,
    remaining: usize,
}
impl Pump {
    fn new(body: &[u8], limits: &Limits) -> Self {
        let mut intake = Intake::with_capacity(
            u32::try_from(body.len()).expect("test bytes").max(tokenizer::largest_demand(&limits.json)),
        );
        intake.append(body).expect("test input");
        Self {
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.json },
            tokenizer: tokenizer::Tokenizer::new(&limits.json),
            above: Queue::with_capacity(1),
            below: Queue::with_capacity(1),
            intake,
            remaining: body.len().saturating_mul(4).saturating_add(32),
        }
    }
    fn next(&mut self, demand: tokenizer::Request) -> tokenizer::Event {
        tokenizer::down(&mut self.tokenizer, &self.env, demand, &mut self.above, &mut self.below);
        while self.remaining > 0 {
            self.remaining = self.remaining.saturating_sub(1);
            if let Some(event) = self.above.pop() {
                return event;
            }
            match self.below.pop().expect("tokenizer demand") {
                Down::Demand { read, room: 0 } => {
                    let up = match self.intake.meet(read) {
                        Some(bytes) => Up::Bytes(bytes),
                        None => Up::End,
                    };
                    tokenizer::up(&mut self.tokenizer, &self.env, up, &mut self.above, &mut self.below);
                }
                other @ (Down::Demand { .. } | Down::Send(_) | Down::Finish) => {
                    panic!("unexpected stream demand {other:?}")
                }
            }
        }
        panic!("decoder exhausted the input step bound")
    }
}
pub fn request(
    method: crate::forgejo::request::Method,
    target: &[u8],
    body: &[u8],
    limits: &Limits,
) -> Result<crate::forgejo::request::Request, Malformed> {
    use crate::forgejo::server;
    let server::Route::Body(mut decoder) = server::route(method, target, limits).unwrap() else { panic!("body route") };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.json };
    let mut tokenizer = tokenizer::Tokenizer::new(&limits.json);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut intake =
        Intake::with_capacity(u32::try_from(body.len()).unwrap().max(tokenizer::largest_demand(&limits.json)));
    intake.append(body).unwrap();
    let mut demand = server::body_first(&decoder);
    let mut read = None;
    for _ in 0..body.len().saturating_mul(4).saturating_add(32) {
        match read.take() {
            Some(read) => {
                let event = match intake.meet(read) {
                    Some(bytes) => Up::Bytes(bytes),
                    None => Up::End,
                };
                tokenizer::up(&mut tokenizer, &env, event, &mut above, &mut below);
            }
            None => tokenizer::down(&mut tokenizer, &env, demand, &mut above, &mut below),
        }
        if let Some(Down::Demand { read: r, room: 0 }) = below.pop() {
            read = Some(r);
        }
        if let Some(event) = above.pop() {
            match server::body(&mut decoder, limits, &event) {
                server::BodyStep::Demand(next) => demand = next,
                server::BodyStep::Decoded(request) => return Ok(request),
                server::BodyStep::Malformed(error) => return Err(error),
            }
        }
    }
    panic!("body decoder exhausted input step bound")
}
pub fn hook_spec() -> crate::forgejo::request::HookSpec {
    crate::forgejo::request::HookSpec {
        url: Box::from(*b"http://127.0.0.1:3000/hooks/1"),
        secret: skein_lib::Secret::new(Box::from(*b"0123456789abcdef")),
        events: Box::from([Box::<[u8]>::from(*b"push")]),
        active: true,
    }
}
pub fn hook_body() -> &'static str {
    r#"{"type":"forgejo","config":{"url":"http://127.0.0.1:3000/hooks/1","content_type":"json","secret":"0123456789abcdef"},"events":["push"],"active":true}"#
}
pub fn debug(value: &impl core::fmt::Debug) -> Box<[u8]> {
    struct Text {
        bytes: [u8; 64],
        len: usize,
    }
    impl core::fmt::Write for Text {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            let end = self.len.checked_add(text.len()).ok_or(core::fmt::Error)?;
            for (to, from) in self.bytes.get_mut(self.len..end).ok_or(core::fmt::Error)?.iter_mut().zip(text.as_bytes())
            {
                *to = *from;
            }
            self.len = end;
            Ok(())
        }
    }
    let mut text = Text { bytes: [0; 64], len: 0 };
    core::fmt::write(&mut text, format_args!("{value:?}")).unwrap();
    Box::from(&text.bytes[..text.len])
}

pub const REPO: crate::api::Repository = crate::api::Repository { forge: 1, repository: 1 };

pub fn specimen() -> crate::forgejo::plans::tables::Repository {
    crate::forgejo::plans::tables::Repository {
        owner: Box::from(&b"fixture"[..]),
        name: Box::from(&b"specimen"[..]),
        object_format: crate::forgejo::id::ObjectFormat::Sha1,
        merge_style: crate::forgejo::document::MergeStyle::Squash,
        html_url: Box::from(&b"http://127.0.0.1:3000/fixture/specimen"[..]),
    }
}
