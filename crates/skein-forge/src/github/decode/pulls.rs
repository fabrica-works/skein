//! Pull and CI scratch rows, reset as each streamed object ends.
use super::{Malformed, walk::Node};
use crate::{
    api,
    forgejo::{id::unsigned, time},
    github::{
        document::{self, Kind, Row},
        id,
    },
};
use alloc::boxed::Box;
use skein_json::Token;
use skein_lib::List;

#[derive(Debug)]
pub(super) struct State {
    pull: document::Pull,
    combined: document::Combined,
    context: document::Context,
    reviewers: List<u64>,
    reviewer: u64,
    inner: u32,
}
fn pull() -> document::Pull {
    document::Pull {
        number: 0,
        closed: false,
        merged: false,
        merge_commit: None,
        mergeable: false,
        head: Box::from([]),
        base: Box::from([]),
        head_commit: [0; 32],
        base_commit: [0; 32],
        reviewers: Box::from([]),
    }
}
fn context() -> document::Context {
    document::Context {
        updated: None,
        context: Box::from([]),
        check: api::Check::Pending,
        description: Box::from([]),
        url: Box::from([]),
    }
}
impl State {
    pub(super) fn new(inner: u32) -> Self {
        Self {
            pull: pull(),
            combined: document::Combined { check: api::Check::Pending, total: 0 },
            context: context(),
            reviewers: List::with_capacity(inner),
            reviewer: 0,
            inner,
        }
    }
    pub(super) fn pull(&self) -> document::Pull {
        self.pull.clone()
    }
    pub(super) fn combined(&self) -> document::Combined {
        self.combined.clone()
    }
    pub(super) fn end(&mut self, node: Node, kind: Kind) -> Result<Option<Row>, Malformed> {
        if node == Node::Reviewer && self.reviewers.push(self.reviewer).is_err() {
            return Err(Malformed::Bound { field: b"requested_reviewers" });
        }
        if node == Node::PullObject {
            self.pull.reviewers = core::mem::replace(&mut self.reviewers, List::with_capacity(self.inner)).into_boxed();
            if kind == Kind::PullRows {
                return Ok(Some(Row::Pull(core::mem::replace(&mut self.pull, pull()))));
            }
        }
        if node == Node::Context {
            return Ok(Some(Row::Context(core::mem::replace(&mut self.context, context()))));
        }
        Ok(None)
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match token {
            Token::Number(text) => {
                let number = unsigned(&text).expect("walker checked integer");
                if node == Node::PullObject {
                    self.pull.number = number;
                } else if node == Node::Reviewer {
                    self.reviewer = number;
                } else {
                    self.combined.total = number;
                }
            }
            Token::True | Token::False => {
                if key == b"merged" {
                    self.pull.merged = token == Token::True;
                } else {
                    self.pull.mergeable = token == Token::True;
                }
            }
            Token::String(text) => {
                if node == Node::PullObject {
                    if key == b"state" {
                        self.pull.closed = match text.as_ref() {
                            b"closed" => true,
                            b"open" => false,
                            _ => return Err(Malformed::Value { field: key }),
                        };
                    } else {
                        self.pull.merge_commit = Some(id::commit(&text).ok_or(Malformed::Id { field: key })?);
                    }
                } else if node == (Node::PullRef { head: true }) || node == (Node::PullRef { head: false }) {
                    let head = node == (Node::PullRef { head: true });
                    if key == b"ref" {
                        if head {
                            self.pull.head = text;
                        } else {
                            self.pull.base = text;
                        }
                    } else {
                        let commit = id::commit(&text).ok_or(Malformed::Id { field: key })?;
                        if head {
                            self.pull.head_commit = commit;
                        } else {
                            self.pull.base_commit = commit;
                        }
                    }
                } else if node == Node::CombinedObject {
                    self.combined.check = check(&text, key)?;
                } else if node == Node::Context {
                    if key == b"context" {
                        self.context.context = text;
                    } else if key == b"description" {
                        self.context.description = text;
                    } else if key == b"target_url" {
                        self.context.url = text;
                    } else if key == b"state" {
                        self.context.check = check(&text, key)?;
                    } else {
                        self.context.updated = Some(time::read(&text).ok_or(Malformed::Time { field: key })?);
                    }
                }
            }
            Token::ObjectStart
            | Token::ObjectEnd
            | Token::ArrayStart
            | Token::ArrayEnd
            | Token::Key(_)
            | Token::Null => unreachable!("pull field table"),
        }
        Ok(())
    }
}
fn check(text: &[u8], field: &'static [u8]) -> Result<api::Check, Malformed> {
    match text {
        b"success" => Ok(api::Check::Passed),
        b"failure" | b"error" => Ok(api::Check::Failed),
        b"pending" => Ok(api::Check::Pending),
        _ => Err(Malformed::Value { field }),
    }
}
