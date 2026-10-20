//! One tokenizer event in, one GitHub decoder step out.
use super::{
    document::{self, Document, Kind, Row},
    limits::Limits,
};
pub use crate::forgejo::decode::Malformed;
use alloc::boxed::Box;
use skein_json::{Token, tokenizer};
use skein_lib::List;
mod app;
pub(crate) mod delivery;
mod issues;
mod walk;
use walk::{Move, Node, Walk};
/// The result of one event, with its next demand returned beside it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Step {
    Demand,
    Row(Row),
    Done(Document),
    Malformed(Malformed),
}
/// Retains only the selected document and one streaming row.
#[derive(Debug)]
pub struct Decoder {
    walk: Walk,
    kind: Kind,
    app: app::State,
    issues: issues::State,
    repository: document::Repository,
    branch: document::Branch,
    message: Box<[u8]>,
    errors: List<Box<[u8]>>,
    error: Box<[u8]>,
    terminal: bool,
}
/// Starts a decoder; its first tokenizer demand is Next.
#[must_use]
pub fn decoder(kind: Kind, limits: &Limits) -> Decoder {
    Decoder {
        walk: Walk::new(kind, limits),
        kind,
        app: app::State::new(kind),
        issues: issues::State::new(limits.inner),
        repository: document::Repository {
            id: 0,
            owner: Box::from([]),
            owner_id: 0,
            organization: false,
            name: Box::from([]),
            default_branch: Box::from([]),
            html_url: Box::from([]),
            merge: false,
            squash: false,
            rebase: false,
        },
        branch: document::Branch { name: Box::from([]), commit: [0; 32] },
        message: Box::from([]),
        errors: List::with_capacity(limits.inner),
        error: Box::from([]),
        terminal: false,
    }
}
impl Decoder {
    /// Consumes one tokenizer event and returns the next demand and progress.
    pub fn step(&mut self, event: tokenizer::Event) -> (tokenizer::Request, Step) {
        if self.terminal {
            return (tokenizer::Request::Close, Step::Malformed(Malformed::Shape { field: b"" }));
        }
        let result = match self.walk.step(event) {
            Ok(movement) => self.movement(movement),
            Err(error) => Err(error),
        };
        match result {
            Ok((demand, step)) => (demand, step),
            Err(error) => {
                self.terminal = true;
                (tokenizer::Request::Close, Step::Malformed(error))
            }
        }
    }
    fn movement(&mut self, movement: Move) -> Result<(tokenizer::Request, Step), Malformed> {
        match movement {
            Move::Demand(demand) => return Ok((demand, Step::Demand)),
            Move::Scalar(node, key, token) => self.scalar(node, key, token)?,
            Move::End(
                node @ (Node::Installation
                | Node::InstallationAccount
                | Node::InstallationRows
                | Node::Minted
                | Node::ReachableEnvelope
                | Node::ReachableRows
                | Node::Reachable
                | Node::AppOwner
                | Node::ViewerEnvelope
                | Node::ViewerData
                | Node::Viewer
                | Node::GraphqlErrors
                | Node::GraphqlError
                | Node::App
                | Node::Hook
                | Node::Conversion
                | Node::BotUser),
            ) => {
                if let Some(row) = self.app.end(node)? {
                    return Ok((tokenizer::Request::Next, Step::Row(row)));
                }
            }
            Move::End(
                node @ (Node::IssueRows
                | Node::Issue
                | Node::IssueUser
                | Node::Labels
                | Node::Label
                | Node::PullRequest
                | Node::CommentRows
                | Node::Comment
                | Node::CommentUser),
            ) => {
                if let Some(row) = self.issues.end(node, self.kind)? {
                    return Ok((tokenizer::Request::Next, Step::Row(row)));
                }
            }
            Move::BodyLong(Some(node), length) => self.issues.long(node, length),
            Move::End(Node::BranchName) => {
                return Ok((
                    tokenizer::Request::Next,
                    Step::Row(Row::Branch(core::mem::replace(&mut self.branch.name, Box::from([])))),
                ));
            }
            Move::End(Node::ErrorEntry) => {
                let error = core::mem::replace(&mut self.error, Box::from([]));
                if self.errors.push(error).is_err() {
                    return Err(Malformed::Bound { field: b"errors" });
                }
            }
            Move::End(
                Node::Repository
                | Node::Owner
                | Node::Branch
                | Node::Commit
                | Node::BranchRows
                | Node::Error
                | Node::Errors,
            ) => {}
            Move::PayloadScalar(..) | Move::PayloadEnd | Move::BodyLong(None, _) => {
                unreachable!("document walker root")
            }
            Move::Done => {
                self.terminal = true;
                let document = match self.kind {
                    Kind::Installation
                    | Kind::InstallationRows
                    | Kind::Minted
                    | Kind::ReachableRows
                    | Kind::Viewer
                    | Kind::App
                    | Kind::HookConfig
                    | Kind::Conversion
                    | Kind::BotUser => self.app.document()?,
                    Kind::Repository => Document::Repository(self.repository.clone()),
                    Kind::Branch => Document::Branch(self.branch.clone()),
                    Kind::BranchRows | Kind::IssueRows | Kind::CommentRows => Document::Rows,
                    Kind::Issue => Document::Issue(self.issues.document()),
                    Kind::Error => Document::Error(document::Refusal {
                        message: core::mem::replace(&mut self.message, Box::from([])),
                        errors: core::mem::replace(&mut self.errors, List::with_capacity(0)).into_boxed(),
                    }),
                };
                return Ok((tokenizer::Request::Close, Step::Done(document)));
            }
        }
        Ok((tokenizer::Request::Next, Step::Demand))
    }
    fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match (node, key, token) {
            (
                node @ (Node::IssueRows
                | Node::Issue
                | Node::IssueUser
                | Node::Labels
                | Node::Label
                | Node::PullRequest
                | Node::CommentRows
                | Node::Comment
                | Node::CommentUser),
                key,
                token,
            ) => return self.issues.scalar(node, key, token),
            (
                node @ (Node::Installation
                | Node::InstallationAccount
                | Node::InstallationRows
                | Node::Minted
                | Node::ReachableEnvelope
                | Node::ReachableRows
                | Node::Reachable
                | Node::AppOwner
                | Node::ViewerEnvelope
                | Node::ViewerData
                | Node::Viewer
                | Node::GraphqlErrors
                | Node::GraphqlError
                | Node::App
                | Node::Hook
                | Node::Conversion
                | Node::BotUser),
                key,
                token,
            ) => return self.app.scalar(node, key, token),
            (Node::Repository, b"id", Token::Number(text)) => {
                self.repository.id = crate::forgejo::id::unsigned(&text).expect("walker checked integer");
            }
            (Node::Owner, b"id", Token::Number(text)) => {
                self.repository.owner_id = crate::forgejo::id::unsigned(&text).expect("walker checked integer");
            }
            (Node::Owner, b"login", Token::String(text)) => self.repository.owner = text,
            (Node::Owner, b"type", Token::String(text)) => {
                self.repository.organization = text.as_ref() == b"Organization";
            }
            (Node::Repository, b"name", Token::String(text)) => self.repository.name = text,
            (Node::Repository, b"default_branch", Token::String(text)) => self.repository.default_branch = text,
            (Node::Repository, b"html_url", Token::String(text)) => self.repository.html_url = text,
            (Node::Repository, b"allow_merge_commit", token) => self.repository.merge = token == Token::True,
            (Node::Repository, b"allow_squash_merge", token) => self.repository.squash = token == Token::True,
            (Node::Repository, b"allow_rebase_merge", token) => self.repository.rebase = token == Token::True,
            (Node::Branch | Node::BranchName, b"name", Token::String(text)) => self.branch.name = text,
            (Node::Commit, b"sha", Token::String(text)) => {
                self.branch.commit = super::id::commit(&text).ok_or(Malformed::Id { field: key })?;
            }
            (Node::Error, b"message", Token::String(text)) => self.message = text,
            (Node::ErrorEntry, b"message", Token::String(text)) => self.error = text,
            (
                Node::Repository
                | Node::Owner
                | Node::Branch
                | Node::BranchName
                | Node::Commit
                | Node::Error
                | Node::ErrorEntry
                | Node::BranchRows
                | Node::Errors,
                _,
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
            ) => unreachable!("GitHub field table"),
        }
        Ok(())
    }
}
