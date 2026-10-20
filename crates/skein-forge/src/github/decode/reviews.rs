//! One review comment scratch row; long bodies are read through.
use super::{Malformed, walk::Node};
use crate::{
    forgejo::{document::Body, id::unsigned},
    github::document::{self, Row},
};
use alloc::boxed::Box;
use skein_json::Token;

#[derive(Debug)]
pub(super) struct State {
    remark: document::Remark,
    line: Option<u32>,
    original: Option<u32>,
}
fn remark() -> document::Remark {
    document::Remark { id: 0, author: 0, path: Box::from([]), line: 0, body: Body::Kept(Box::from([])) }
}
impl State {
    pub(super) fn new() -> Self {
        Self { remark: remark(), line: None, original: None }
    }
    pub(super) fn end(&mut self, node: Node) -> Option<Row> {
        if node == Node::Remark {
            let mut row = core::mem::replace(&mut self.remark, remark());
            row.line = match (self.line.take(), self.original.take()) {
                (Some(line), _) | (None, Some(line)) => line,
                (None, None) => 0,
            };
            return Some(Row::Remark(row));
        }
        None
    }
    pub(super) fn long(&mut self, node: Node, length: u64) {
        if node == Node::Remark {
            self.remark.body = Body::Long(length);
        }
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match token {
            Token::Number(text) => {
                let value = unsigned(&text).ok_or(Malformed::Range { field: key })?;
                if node == Node::RemarkUser {
                    self.remark.author = value;
                } else if key == b"id" {
                    self.remark.id = value;
                } else {
                    let line = u32::try_from(value).ok().ok_or(Malformed::Range { field: key })?;
                    if key == b"line" {
                        self.line = Some(line);
                    } else {
                        self.original = Some(line);
                    }
                }
            }
            Token::String(text) => {
                if key == b"path" {
                    self.remark.path = text;
                } else {
                    self.remark.body = Body::Kept(text);
                }
            }
            Token::ObjectStart
            | Token::ObjectEnd
            | Token::ArrayStart
            | Token::ArrayEnd
            | Token::Key(_)
            | Token::True
            | Token::False
            | Token::Null => return Err(Malformed::Shape { field: key }),
        }
        Ok(())
    }
}
