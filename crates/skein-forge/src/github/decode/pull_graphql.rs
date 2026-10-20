//! The shared pull query's identity and queue fields (forge-github.md, 4.3).
use super::{Malformed, document};
use crate::api;
use alloc::boxed::Box;
use skein_json::Token;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Node {
    EnqueueEnvelope,
    EnqueueData,
    Enqueue,
    Envelope,
    Data,
    Repository,
    Pull,
    Commit,
    Queue,
    Timeline,
    Nodes,
    Removal,
}
#[derive(Debug)]
pub(super) struct State {
    pull: document::PullGraphql,
    present: bool,
    removals: u32,
    limit: u32,
}
impl State {
    pub(super) fn new(inner: u32) -> Self {
        Self {
            pull: document::PullGraphql { id: Box::from([]), merge_commit: None, queued: None, dequeued: None },
            present: false,
            removals: 0,
            limit: inner.min(1),
        }
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match (node, token) {
            (Node::Pull, Token::String(text)) => self.pull.id = text,
            (Node::Commit, Token::String(text)) => {
                self.pull.merge_commit = Some(super::super::id::commit(&text).ok_or(Malformed::Id { field: key })?);
            }
            (Node::Queue, Token::Number(text)) => {
                let number = crate::forgejo::id::unsigned(&text).ok_or(Malformed::Range { field: key })?;
                self.pull.queued = Some(u32::try_from(number).ok().ok_or(Malformed::Range { field: key })?);
            }
            (Node::Removal, Token::String(text)) => {
                let mut end = text.len().min(api::DEQUEUED_BYTES);
                // A UTF-8 code point has at most four bytes.
                for _ in 0u8..4 {
                    let boundary = match text.get(end) {
                        Some(byte) => byte & 0xc0 != 0x80,
                        None => true,
                    };
                    if boundary {
                        break;
                    }
                    end = end.checked_sub(1).ok_or(Malformed::Range { field: key })?;
                }
                self.pull.dequeued =
                    Some(skein_lib::bytes::copy_of(text.get(..end).ok_or(Malformed::Range { field: key })?));
            }
            (
                Node::EnqueueEnvelope
                | Node::EnqueueData
                | Node::Enqueue
                | Node::Envelope
                | Node::Data
                | Node::Repository
                | Node::Pull
                | Node::Commit
                | Node::Queue
                | Node::Timeline
                | Node::Nodes
                | Node::Removal,
                Token::ObjectStart
                | Token::ObjectEnd
                | Token::ArrayStart
                | Token::ArrayEnd
                | Token::Key(_)
                | Token::String(_)
                | Token::Number(_)
                | Token::True
                | Token::False
                | Token::Null,
            ) => return Err(Malformed::Shape { field: key }),
        }
        Ok(())
    }
    pub(super) fn end(&mut self, node: Node) -> Result<(), Malformed> {
        match node {
            Node::Pull => self.present = true,
            Node::Removal => {
                self.removals = self.removals.checked_add(1).ok_or(Malformed::Bound { field: b"nodes" })?;
                if self.removals > self.limit {
                    return Err(Malformed::Bound { field: b"nodes" });
                }
            }
            Node::EnqueueEnvelope
            | Node::EnqueueData
            | Node::Enqueue
            | Node::Envelope
            | Node::Data
            | Node::Repository
            | Node::Commit
            | Node::Queue
            | Node::Timeline
            | Node::Nodes => {}
        }
        Ok(())
    }
    pub(super) fn enqueued(&self) -> Result<document::Document, Malformed> {
        Ok(document::Document::Enqueued(self.pull.queued.ok_or(Malformed::Missing { field: b"mergeQueueEntry" })?))
    }
    pub(super) fn document(&self) -> Result<document::Document, Malformed> {
        if !self.present {
            return Err(Malformed::Missing { field: b"pullRequest" });
        }
        Ok(document::Document::PullGraphql(self.pull.clone()))
    }
}
