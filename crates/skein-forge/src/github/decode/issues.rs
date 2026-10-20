//! One issue and comment scratch row, reset at each streamed object end.
use super::{Malformed, walk::Node};
use crate::{
    api,
    forgejo::{document::Body, id::unsigned, time},
    github::document::{self, Kind, Row},
};
use alloc::boxed::Box;
use skein_json::Token;
use skein_lib::{List, Wall};

#[derive(Debug)]
pub(super) struct State {
    issue: document::Issue,
    comment: document::Comment,
    labels: List<Box<[u8]>>,
    label: Box<[u8]>,
    inner: u32,
}
fn issue() -> document::Issue {
    document::Issue {
        number: 0,
        pull: false,
        merged: false,
        state: api::State::Open,
        author: 0,
        labels: Box::from([]),
        updated_at: Wall::EPOCH,
        title: Box::from([]),
        body: Body::Kept(Box::from([])),
    }
}
fn comment() -> document::Comment {
    document::Comment { id: 0, author: 0, created_at: None, updated_at: None, body: Body::Kept(Box::from([])) }
}
impl State {
    pub(super) fn new(inner: u32) -> Self {
        Self { issue: issue(), comment: comment(), labels: List::with_capacity(inner), label: Box::from([]), inner }
    }
    pub(super) fn document(&self) -> document::Issue {
        self.issue.clone()
    }
    pub(super) fn long(&mut self, node: Node, length: u64) {
        if node == Node::Issue {
            self.issue.body = Body::Long(length);
        } else if node == Node::Comment {
            self.comment.body = Body::Long(length);
        }
    }
    pub(super) fn end(&mut self, node: Node, kind: Kind) -> Result<Option<Row>, Malformed> {
        if node == Node::PullRequest {
            self.issue.pull = true;
        }
        if node == Node::Label {
            let label = core::mem::replace(&mut self.label, Box::from([]));
            if self.labels.push(label).is_err() {
                return Err(Malformed::Bound { field: b"labels" });
            }
        }
        if node == Node::Issue {
            self.issue.labels = core::mem::replace(&mut self.labels, List::with_capacity(self.inner)).into_boxed();
            if self.issue.merged {
                self.issue.state = api::State::Closed;
            }
            if kind == Kind::IssueRows {
                return Ok(Some(Row::Issue(core::mem::replace(&mut self.issue, issue()))));
            }
        }
        if node == Node::Comment {
            return Ok(Some(Row::Comment(core::mem::replace(&mut self.comment, comment()))));
        }
        Ok(None)
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match token {
            Token::Number(text) => {
                let value = unsigned(&text).expect("walker checked integer");
                if (node == Node::Issue || node == Node::Comment) && value == 0 {
                    return Err(Malformed::Range { field: key });
                }
                if node == Node::Issue {
                    self.issue.number = value;
                } else if node == Node::IssueUser {
                    self.issue.author = value;
                } else if node == Node::Comment {
                    self.comment.id = value;
                } else if node == Node::CommentUser {
                    self.comment.author = value;
                }
            }
            Token::String(text) => {
                if key == b"state" {
                    self.issue.state = match text.as_ref() {
                        b"open" => api::State::Open,
                        b"closed" => api::State::Closed,
                        _ => return Err(Malformed::Value { field: key }),
                    };
                } else if key == b"body" {
                    if node == Node::Issue {
                        self.issue.body = Body::Kept(text);
                    } else {
                        self.comment.body = Body::Kept(text);
                    }
                } else if key == b"title" {
                    self.issue.title = text;
                } else if node == Node::Label {
                    self.label = text;
                } else {
                    let wall = time::read(&text).ok_or(Malformed::Time { field: key })?;
                    if node == Node::Issue {
                        self.issue.updated_at = wall;
                    } else if key == b"created_at" {
                        self.comment.created_at = Some(wall);
                    } else {
                        self.comment.updated_at = Some(wall);
                    }
                }
            }
            Token::True => self.issue.merged = true,
            Token::ObjectStart
            | Token::ObjectEnd
            | Token::ArrayStart
            | Token::ArrayEnd
            | Token::Key(_)
            | Token::False
            | Token::Null => unreachable!("issue field table"),
        }
        Ok(())
    }
}
