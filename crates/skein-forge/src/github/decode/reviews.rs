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
    review: document::Review,
    remark: document::Remark,
    line: Option<u32>,
    original: Option<u32>,
}
fn remark() -> document::Remark {
    document::Remark { id: 0, author: 0, path: Box::from([]), line: 0, body: Body::Kept(Box::from([])) }
}
impl State {
    pub(super) fn new() -> Self {
        Self { review: review(), remark: remark(), line: None, original: None }
    }
    pub(super) fn end(&mut self, node: Node) -> Option<Row> {
        if node == Node::Review {
            return Some(Row::Review(core::mem::replace(&mut self.review, review())));
        }
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
        } else if node == Node::Review {
            self.review.body = Body::Long(length);
        }
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        if node == Node::Review || node == Node::ReviewUser {
            return self.review_scalar(node, key, token);
        }
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

fn review() -> document::Review {
    document::Review {
        id: 0,
        author: 0,
        verdict: None,
        commit: [0; 32],
        body: Body::Kept(Box::from([])),
        submitted: None,
        official: false,
    }
}
impl State {
    fn review_scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match token {
            Token::Number(text) => {
                let value = unsigned(&text).ok_or(Malformed::Range { field: key })?;
                if node == Node::ReviewUser {
                    self.review.author = value;
                } else {
                    self.review.id = value;
                }
            }
            Token::String(text) => match key {
                b"state" => {
                    self.review.verdict = match text.as_ref() {
                        b"APPROVED" => Some(crate::api::Verdict::Approve),
                        b"CHANGES_REQUESTED" => Some(crate::api::Verdict::RequestChanges),
                        b"COMMENTED" | b"DISMISSED" => Some(crate::api::Verdict::Comment),
                        b"PENDING" => None,
                        _ => return Err(Malformed::Value { field: key }),
                    };
                }
                b"commit_id" => {
                    self.review.commit = crate::github::id::commit(&text).ok_or(Malformed::Id { field: key })?;
                }
                b"body" => self.review.body = Body::Kept(text),
                b"submitted_at" => {
                    self.review.submitted =
                        Some(crate::forgejo::time::read(&text).ok_or(Malformed::Time { field: key })?);
                }
                b"author_association" => {
                    self.review.official =
                        text.as_ref() == b"OWNER" || text.as_ref() == b"MEMBER" || text.as_ref() == b"COLLABORATOR";
                }
                _ => return Err(Malformed::Shape { field: key }),
            },
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
