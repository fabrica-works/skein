//! Canonical JSON over skein-json's tokenizer and sized writer (lab.md, 2.2).
use skein_json::{Token, tokenizer as read, writer};
use skein_lib::{Env, Queue, Time, Wall, stream};
use std::collections::{BTreeMap, VecDeque};

pub const LINE: usize = 1 << 20;
const EXACT: i64 = (1_i64 << 53) - 1;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Object(BTreeMap<String, Value>),
    Array(Vec<Value>),
    String(String),
    Number(i64),
    Bool(bool),
    Null,
}
pub fn object(entries: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub fn string(text: impl Into<String>) -> Value {
    Value::String(text.into())
}
pub fn bytes(text: &[u8]) -> Value {
    string(String::from_utf8_lossy(text))
}
pub fn number(n: impl TryInto<i64>) -> Value {
    Value::Number(n.try_into().ok().expect("bounded integer"))
}
pub fn encode(value: &Value) -> Vec<u8> {
    let limits = writer::Limits { depth: 16, length: 32 << 20 };
    let mut size = writer::Encoder::measure(&limits);
    emit(&mut size, value);
    let mut out = writer::Encoder::write(size.measured().expect("bounded answer"), &limits);
    emit(&mut out, value);
    out.finish().into_vec()
}
fn emit(out: &mut writer::Encoder, value: &Value) {
    match value {
        Value::Object(members) => {
            out.object_start();
            for (key, value) in members {
                out.key(key.as_bytes());
                emit(out, value);
            }
            out.object_end();
        }
        Value::Array(values) => {
            out.array_start();
            for value in values {
                emit(out, value);
            }
            out.array_end();
        }
        Value::String(text) => out.string(text.as_bytes()),
        Value::Number(n) => out.number(n.to_string().as_bytes()),
        Value::Bool(value) => out.boolean(*value),
        Value::Null => out.null(),
    }
}
pub fn parse(input: &[u8]) -> Result<Value, String> {
    if input.len() > LINE {
        return Err("call line exceeds 1 MiB".into());
    }
    let limits = read::Limits { depth: 8, string: 1 << 20, number: 32, chunk: 4096, length: (1 << 20) + 1 };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut machine = read::Tokenizer::new(&limits);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut tokens = VecDeque::new();
    let mut at = 0;
    read::down(&mut machine, &env, read::Request::Next, &mut above, &mut below);
    loop {
        if let Some(event) = above.pop() {
            match event {
                read::Event::Token(token) => {
                    tokens.push_back(token);
                    read::down(&mut machine, &env, read::Request::Next, &mut above, &mut below);
                }
                read::Event::Done => break,
                read::Event::Failed(error) => return Err(format!("invalid JSON: {error:?}")),
                read::Event::Long(_) | read::Event::Skipped(_) | read::Event::Closed => {
                    return Err("invalid JSON event".into());
                }
            }
            continue;
        }
        let Some(stream::Down::Demand { read, .. }) = below.pop() else {
            return Err("invalid JSON demand".into());
        };
        let remaining = &input[at..];
        let count = match read {
            stream::Read::Fill(n) => usize::try_from(n).expect("read fits"),
            stream::Read::Scan { until, max } => {
                let max = usize::try_from(max).expect("scan fits").min(remaining.len());
                skein_lib::bytes::find(&remaining[..max], until.as_bytes()).map_or(max, |n| n + until.as_bytes().len())
            }
            stream::Read::Nothing | stream::Read::Line { .. } | stream::Read::Arrived { .. } => {
                return Err("invalid JSON read".into());
            }
        };
        let up = if remaining.is_empty() {
            stream::Up::End
        } else {
            let count = count.min(remaining.len());
            let piece = remaining[..count].into();
            at += count;
            stream::Up::Bytes(piece)
        };
        read::up(&mut machine, &env, up, &mut above, &mut below);
    }
    let value = from_tokens(&mut tokens)?;
    if !tokens.is_empty() {
        return Err("trailing JSON".into());
    }
    Ok(value)
}
fn from_tokens(tokens: &mut VecDeque<Token>) -> Result<Value, String> {
    match tokens.pop_front().ok_or("missing JSON value")? {
        Token::ObjectStart => {
            let mut values = BTreeMap::new();
            loop {
                match tokens.pop_front().ok_or("unclosed object")? {
                    Token::ObjectEnd => break,
                    Token::Key(key) => {
                        let key = String::from_utf8(key.into_vec()).map_err(|e| e.to_string())?;
                        if !key.is_ascii() {
                            return Err("non-ASCII key".into());
                        }
                        if values.insert(key, from_tokens(tokens)?).is_some() {
                            return Err("duplicate key".into());
                        }
                    }
                    other @ (Token::ObjectStart
                    | Token::ArrayStart
                    | Token::ArrayEnd
                    | Token::String(_)
                    | Token::Number(_)
                    | Token::True
                    | Token::False
                    | Token::Null) => return Err(format!("expected key: {other:?}")),
                }
            }
            Ok(Value::Object(values))
        }
        Token::ArrayStart => {
            let mut values = Vec::new();
            while tokens.front() != Some(&Token::ArrayEnd) {
                values.push(from_tokens(tokens)?);
            }
            tokens.pop_front();
            Ok(Value::Array(values))
        }
        Token::String(s) => Ok(string(String::from_utf8(s.into_vec()).map_err(|e| e.to_string())?)),
        Token::Number(n) => {
            let n = std::str::from_utf8(&n).map_err(|e| e.to_string())?.parse::<i64>().map_err(|e| e.to_string())?;
            if !(-EXACT..=EXACT).contains(&n) {
                return Err("integer outside exact range".into());
            }
            Ok(number(n))
        }
        Token::True => Ok(Value::Bool(true)),
        Token::False => Ok(Value::Bool(false)),
        Token::Null => Ok(Value::Null),
        other @ (Token::ObjectEnd | Token::ArrayEnd | Token::Key(_)) => Err(format!("expected value: {other:?}")),
    }
}
