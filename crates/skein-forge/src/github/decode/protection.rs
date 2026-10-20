//! Rule rows stream; contexts are bounded by the dialect's inner limit.
use super::Malformed;
use crate::{
    api::Bypass,
    forgejo::id::unsigned,
    github::document::{self, RuleKind},
};
use alloc::boxed::Box;
use skein_json::Token;
use skein_lib::List;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Node {
    Rows,
    Rule,
    Ruleset,
    Parameters,
    Checks,
    Check,
    Classic,
    ClassicChecks,
    Contexts,
    Reviews,
    Signatures,
    Linear,
}
#[derive(Debug)]
pub(super) struct State {
    ruleset: u64,
    kind: Box<[u8]>,
    value: document::Protection,
    contexts: List<Box<[u8]>>,
    bypass: Bypass,
    fields: u8,
    inner: u32,
}
impl State {
    pub(super) fn new(inner: u32) -> Self {
        Self {
            ruleset: 0,
            kind: Box::from([]),
            value: document::Protection::empty(),
            contexts: List::with_capacity(0),
            bypass: Bypass::Never,
            fields: 0,
            inner,
        }
    }
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        if node == Node::Ruleset {
            let Token::String(text) = token else {
                return Err(Malformed::Shape { field: key });
            };
            self.bypass = match text.as_ref() {
                b"never" => Bypass::Never,
                b"pull_requests_only" => Bypass::PullsOnly,
                b"always" => Bypass::Always,
                _ => return Err(Malformed::Value { field: key }),
            };
        } else if node == Node::Rule && key == b"ruleset_id" {
            let Token::Number(text) = token else {
                return Err(Malformed::Shape { field: key });
            };
            self.ruleset = unsigned(&text).ok_or(Malformed::Range { field: key })?;
            if self.ruleset == 0 {
                return Err(Malformed::Range { field: key });
            }
        } else if node == Node::Rule {
            let Token::String(text) = token else {
                return Err(Malformed::Shape { field: key });
            };
            self.kind = text;
        } else if node == Node::Contexts || node == Node::Check {
            let Token::String(text) = token else {
                return Err(Malformed::Shape { field: key });
            };
            if self.contexts.capacity() == 0 {
                self.contexts = List::with_capacity(self.inner);
            }
            self.contexts.push(text).or(Err(Malformed::Bound { field: b"contexts" }))?;
        } else if key == b"required_approving_review_count" {
            let Token::Number(text) = token else {
                return Err(Malformed::Shape { field: key });
            };
            self.value.approvals = u32::try_from(unsigned(&text).ok_or(Malformed::Range { field: key })?)
                .or(Err(Malformed::Range { field: key }))?;
            self.fields |= 1;
        } else if key == b"dismiss_stale_reviews_on_push" || key == b"dismiss_stale_reviews" {
            self.value.dismiss_stale = token == Token::True;
            self.fields |= 2;
        } else if key == b"require_code_owner_review" || key == b"require_code_owner_reviews" {
            self.value.code_owners = token == Token::True;
            self.fields |= 4;
        } else if node == Node::Signatures {
            self.value.signatures = token == Token::True;
        } else if node == Node::Linear {
            self.value.linear = token == Token::True;
        } else {
            return Err(Malformed::Shape { field: key });
        }
        Ok(())
    }
    pub(super) fn end(&mut self, node: Node) -> Result<Option<document::Row>, Malformed> {
        if node == Node::Checks {
            self.fields |= 8;
        }
        if node == Node::ClassicChecks || node == Node::Reviews || node == Node::Signatures || node == Node::Linear {
            self.fields |= 16;
        }
        if node != Node::Rule {
            return Ok(None);
        }
        let kind = match self.kind.as_ref() {
            b"pull_request" => {
                if self.fields & 7 != 7 {
                    return Err(Malformed::Missing { field: b"parameters" });
                }
                RuleKind::PullRequest {
                    approvals: self.value.approvals,
                    dismiss_stale: self.value.dismiss_stale,
                    code_owners: self.value.code_owners,
                }
            }
            b"required_status_checks" => {
                if self.fields & 8 == 0 {
                    return Err(Malformed::Missing { field: b"required_status_checks" });
                }
                RuleKind::StatusChecks(core::mem::replace(&mut self.contexts, List::with_capacity(0)).into_boxed())
            }
            b"merge_queue" => RuleKind::Queue,
            b"required_signatures" => RuleKind::Signatures,
            b"required_linear_history" => RuleKind::Linear,
            _ => RuleKind::Skipped,
        };
        self.value = document::Protection::empty();
        self.contexts = List::with_capacity(0);
        self.fields = 0;
        Ok(Some(document::Row::Rule(document::Rule { ruleset: self.ruleset, kind })))
    }
    pub(super) fn ruleset(&self) -> Bypass {
        self.bypass
    }
    pub(super) fn classic(&mut self) -> Option<document::Protection> {
        if self.fields & 16 == 0 {
            return None;
        }
        self.value.contexts = core::mem::replace(&mut self.contexts, List::with_capacity(0)).into_boxed();
        Some(core::mem::replace(&mut self.value, document::Protection::empty()))
    }
}
