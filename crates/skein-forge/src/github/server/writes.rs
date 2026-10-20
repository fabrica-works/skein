//! Bounded write request bodies and minimal mutation answers.
use super::{Limits, Request, output_limits};
use alloc::boxed::Box;
use skein_json::{
    Token,
    tokenizer::{self, Event},
    writer::Encoder,
};
use skein_lib::{
    Env, Intake, List, Overflow, Queue, Time, Wall, Writer,
    stream::{Down, Up},
};

/// Writes the minimal JSON object accepted by Done-result mutations.
pub fn done(out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    measure.object_start();
    measure.object_end();
    let Ok(length) = measure.measured() else {
        return Err(Overflow);
    };
    let mut json = Encoder::write(length, &limits);
    json.object_start();
    json.object_end();
    out.put(&json.finish())
}
#[derive(Debug)]
struct Field {
    key: &'static [u8],
    body: bool,
    required: bool,
}
#[derive(Debug)]
struct Value {
    key: &'static [u8],
    text: Box<[u8]>,
}
#[derive(Debug)]
struct Fields {
    values: List<Value>,
}
impl Fields {
    fn get(&self, key: &[u8]) -> Option<&[u8]> {
        for value in &self.values {
            if value.key == key {
                return Some(&value.text);
            }
        }
        None
    }
}
#[derive(Debug)]
enum Want {
    Start,
    Key,
    Value(&'static [u8], u32),
    Unknown(u32),
    End,
    Done,
}
#[derive(Debug)]
struct Parser {
    want: Want,
    fields: Fields,
}
impl Parser {
    #[expect(clippy::too_many_lines, reason = "the bounded tokenizer states exhaustively handle every event")]
    fn event(&mut self, event: Event, specs: &[Field], limits: &Limits) -> Option<tokenizer::Request> {
        let want = core::mem::replace(&mut self.want, Want::Done);
        match want {
            Want::Start => {
                if event != Event::Token(Token::ObjectStart) {
                    return None;
                }
                self.want = Want::Key;
            }
            Want::Key => match event {
                Event::Token(Token::ObjectEnd) => {
                    for field in specs {
                        if field.required && self.fields.get(field.key).is_none() {
                            return None;
                        }
                    }
                    self.want = Want::End;
                }
                Event::Token(Token::Key(key)) => {
                    for field in specs {
                        if field.key == key.as_ref() {
                            if self.fields.get(field.key).is_some() {
                                return None;
                            }
                            let cap = if field.body { limits.body_bytes } else { limits.string_bytes };
                            self.want = Want::Value(field.key, cap);
                            return Some(tokenizer::Request::Text(cap));
                        }
                    }
                    self.want = Want::Unknown(0);
                    return Some(tokenizer::Request::Text(0));
                }
                Event::Token(
                    Token::ObjectStart
                    | Token::ArrayStart
                    | Token::ArrayEnd
                    | Token::String(_)
                    | Token::Number(_)
                    | Token::True
                    | Token::False
                    | Token::Null,
                )
                | Event::Long(_)
                | Event::Skipped(_)
                | Event::Done
                | Event::Failed(_)
                | Event::Closed => return None,
            },
            Want::Value(key, cap) => {
                let text = match event {
                    Event::Token(Token::String(text)) => text,
                    Event::Token(
                        Token::ObjectStart
                        | Token::ObjectEnd
                        | Token::ArrayStart
                        | Token::ArrayEnd
                        | Token::Key(_)
                        | Token::Number(_)
                        | Token::True
                        | Token::False
                        | Token::Null,
                    )
                    | Event::Long(_)
                    | Event::Skipped(_)
                    | Event::Done
                    | Event::Failed(_)
                    | Event::Closed => return None,
                };
                if u64::try_from(text.len()).ok()? > u64::from(cap) {
                    return None;
                }
                self.fields.values.push(Value { key, text }).ok()?;
                self.want = Want::Key;
            }
            Want::Unknown(depth) => {
                let depth = match event {
                    Event::Token(Token::ObjectStart | Token::ArrayStart) => depth.checked_add(1)?,
                    Event::Token(Token::ObjectEnd | Token::ArrayEnd) => depth.checked_sub(1)?,
                    Event::Token(Token::Key(_)) => {
                        self.want = Want::Unknown(depth);
                        return Some(tokenizer::Request::Text(0));
                    }
                    Event::Token(Token::String(_) | Token::Number(_) | Token::True | Token::False | Token::Null)
                    | Event::Long(_) => depth,
                    Event::Skipped(_) | Event::Done | Event::Failed(_) | Event::Closed => return None,
                };
                if depth == 0 {
                    self.want = Want::Key;
                } else {
                    self.want = Want::Unknown(depth);
                    return Some(tokenizer::Request::Text(0));
                }
            }
            Want::End => {
                if event != Event::Done {
                    return None;
                }
                self.want = Want::Done;
                return Some(tokenizer::Request::Close);
            }
            Want::Done => return None,
        }
        Some(tokenizer::Request::Next)
    }
}
fn parse(body: &[u8], specs: &[Field], limits: &Limits) -> Option<Fields> {
    if u64::try_from(body.len()).ok()? > u64::from(limits.body_bytes) {
        return None;
    }
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.json };
    let mut tokenizer = tokenizer::Tokenizer::new(&limits.json);
    let mut parser =
        Parser { want: Want::Start, fields: Fields { values: List::with_capacity(u32::try_from(specs.len()).ok()?) } };
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut intake =
        Intake::with_capacity(u32::try_from(body.len()).ok()?.max(tokenizer::largest_demand(&limits.json)));
    intake.append(body).ok()?;
    tokenizer::down(&mut tokenizer, &env, tokenizer::Request::Next, &mut above, &mut below);
    for _ in 0..body.len().checked_mul(8)?.checked_add(64)? {
        if let Some(event) = above.pop() {
            let demand = parser.event(event, specs, limits)?;
            if demand == tokenizer::Request::Close {
                return Some(parser.fields);
            }
            tokenizer::down(&mut tokenizer, &env, demand, &mut above, &mut below);
        } else {
            let read = match below.pop()? {
                Down::Demand { read, room: 0 } => read,
                Down::Demand { .. } | Down::Send(_) | Down::Finish => return None,
            };
            let event = match intake.meet(read) {
                Some(bytes) => Up::Bytes(bytes),
                None => Up::End,
            };
            tokenizer::up(&mut tokenizer, &env, event, &mut above, &mut below);
        }
    }
    None
}
/// Decodes a write's body separately from route dispatch; path fields come from the caller.
#[must_use]
pub fn write_body(request: Request, body: &[u8], limits: &Limits) -> Option<Request> {
    match request {
        Request::Close { owner, name, number } => {
            let fields = parse(body, &[Field { key: b"state", body: false, required: true }], limits)?;
            if fields.get(b"state")? != b"closed" {
                return None;
            }
            Some(Request::Close { owner, name, number })
        }
        Request::DeleteBranch { .. } => {
            if body.is_empty() {
                Some(request)
            } else {
                None
            }
        }
        Request::Installations { .. }
        | Request::Installation { .. }
        | Request::Mint { .. }
        | Request::Reachable { .. }
        | Request::Viewer
        | Request::App
        | Request::HookConfig
        | Request::SetHook { .. }
        | Request::Uninstall { .. }
        | Request::Conversion { .. }
        | Request::BotUser { .. }
        | Request::Repository { .. }
        | Request::Locate { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Branches { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::Raw { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::User { .. } => None,
    }
}
