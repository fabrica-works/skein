//! Log rows and the trusted wrapper around the shared pushed-payload decoder.
use super::{Malformed, delivery, walk::Node};
use crate::{
    forgejo::id::unsigned,
    github::{guid, limits::Limits, log},
};
use alloc::boxed::Box;
use skein_json::{
    Token,
    tokenizer::{Event, Request},
};

#[derive(Debug)]
pub(super) struct State {
    row: log::Row,
    event: Option<Box<[u8]>>,
    header: Option<Box<[u8]>>,
    depth: u32,
    payload: Option<Box<delivery::Decoder>>,
    decoded: Option<crate::github::Delivery>,
    limits: Limits,
}
impl State {
    pub(super) fn new(limits: &Limits) -> Self {
        Self { row: row(), event: None, header: None, depth: 0, payload: None, decoded: None, limits: *limits }
    }
    pub(super) fn active(&self) -> bool {
        self.depth != 0
    }
    pub(super) fn start(&mut self, token: Token) -> Result<Request, Malformed> {
        // forge-github.md, 9.3: "trusted by its channel". The re-encoded
        // payload is decoded without checking an HMAC signature.
        self.payload = match delivery::decoder(&self.row.event, &self.limits) {
            Some(decoder) => Some(Box::new(decoder)),
            None => {
                self.decoded = Some(crate::github::Delivery::Other);
                None
            }
        };
        self.depth = 1;
        self.feed(Event::Token(token))
    }
    pub(super) fn payload(&mut self, event: Event) -> Result<Request, Malformed> {
        match &event {
            Event::Token(Token::ObjectStart | Token::ArrayStart) => {
                self.depth = self.depth.checked_add(1).ok_or(Malformed::Bound { field: b"payload" })?;
            }
            Event::Token(Token::ObjectEnd | Token::ArrayEnd) => {
                self.depth = self.depth.checked_sub(1).ok_or(Malformed::Shape { field: b"payload" })?;
            }
            Event::Failed(error) => return Err(json_error(*error)),
            Event::Token(
                Token::Key(_) | Token::String(_) | Token::Number(_) | Token::True | Token::False | Token::Null,
            )
            | Event::Long(_) => {}
            Event::Skipped(_) | Event::Done | Event::Closed => return Err(Malformed::Shape { field: b"payload" }),
        }
        let demand = self.feed(event)?;
        if self.depth != 0 {
            return Ok(demand);
        }
        if let Some(decoder) = &mut self.payload {
            self.decoded = Some(finish(decoder)?);
        }
        self.payload = None;
        Ok(Request::Next)
    }
    fn feed(&mut self, event: Event) -> Result<Request, Malformed> {
        if let Some(decoder) = &mut self.payload {
            return match decoder.step(event) {
                delivery::Step::Demand(demand) => Ok(demand),
                delivery::Step::Malformed(error) => Err(error),
                delivery::Step::Done(_) => Err(Malformed::Shape { field: b"payload" }),
            };
        }
        Ok(Request::Text(0))
    }
    pub(super) fn expected(&mut self, event: Box<[u8]>) {
        self.row.event = event;
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match token {
            Token::Number(text) => {
                let value = unsigned(&text).ok_or(Malformed::Range { field: key })?;
                match key {
                    b"id" => self.row.id = value,
                    b"installation_id" => self.row.installation_id = Some(value),
                    b"repository_id" => self.row.repository_id = Some(value),
                    b"status_code" => {
                        self.row.status_code = u16::try_from(value).or(Err(Malformed::Range { field: key }))?;
                    }
                    _ => unreachable!("log numeric field table"),
                }
            }
            Token::String(text) => match key {
                b"guid" | b"X-GitHub-Delivery" => {
                    let value = guid::read(&text).ok_or(Malformed::Value { field: key })?;
                    self.row.guid = value;
                }
                b"event" if node == Node::LogRow => self.row.event = text,
                b"event" => self.event = Some(text),
                b"X-GitHub-Event" => self.header = Some(text),
                b"action" => self.row.action = Some(text),
                b"delivered_at" => self.row.delivered_at = super::app::time(&text, key)?,
                _ => unreachable!("log text field table"),
            },
            Token::True | Token::False => self.row.redelivery = token == Token::True,
            Token::ObjectStart
            | Token::ObjectEnd
            | Token::ArrayStart
            | Token::ArrayEnd
            | Token::Key(_)
            | Token::Null => unreachable!("log scalar field table"),
        }
        Ok(())
    }
    pub(super) fn row(&mut self) -> log::Row {
        let mut row = core::mem::replace(&mut self.row, row());
        if row.event.as_ref() == b"ping"
            || row.event.as_ref() == b"installation"
            || row.event.as_ref() == b"installation_repositories"
        {
            row.installation_id = None;
            row.repository_id = None;
        }
        row
    }
    pub(super) fn document(&mut self) -> Result<log::Logged, Malformed> {
        let delivery = self.delivery()?;
        Ok(log::Logged { event: self.event.take().expect("checked event"), delivery })
    }
    fn delivery(&mut self) -> Result<crate::github::Delivery, Malformed> {
        let event = self.event.as_ref().ok_or(Malformed::Missing { field: b"event" })?;
        if event != &self.row.event {
            return Err(Malformed::Value { field: b"event" });
        }
        if self.header.as_ref() != Some(event) {
            return Err(Malformed::Value { field: b"X-GitHub-Event" });
        }
        self.decoded.take().ok_or(Malformed::Missing { field: b"payload" })
    }
}
fn finish(decoder: &mut delivery::Decoder) -> Result<crate::github::Delivery, Malformed> {
    match decoder.step(Event::Done) {
        delivery::Step::Done(value) => Ok(value),
        delivery::Step::Malformed(error) => Err(error),
        delivery::Step::Demand(_) => Err(Malformed::Shape { field: b"payload" }),
    }
}
pub(super) fn json_error(error: skein_json::tokenizer::Error) -> Malformed {
    if error == skein_json::tokenizer::Error::TooLong {
        Malformed::Bound { field: b"payload" }
    } else {
        Malformed::Json(error)
    }
}
fn row() -> log::Row {
    log::Row {
        id: 0,
        guid: [0; 16],
        event: Box::from([]),
        action: None,
        installation_id: None,
        repository_id: None,
        status_code: 0,
        delivered_at: skein_lib::Wall::EPOCH,
        redelivery: false,
    }
}
