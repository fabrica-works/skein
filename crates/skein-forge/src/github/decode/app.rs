//! Retained App fields, with secrets wrapped at their scalar event.
use super::super::document::{self, Document, Kind, Row};
use super::{Malformed, walk::Node};
use alloc::boxed::Box;
use skein_json::Token;
use skein_lib::{Secret, Wall};

#[derive(Debug)]
pub(super) struct State {
    kind: Kind,
    installation: document::Installation,
    minted: document::Minted,
    reachable: document::Reachable,
    viewer: document::Viewer,
    graphql: document::GraphqlRefusal,
    app: document::App,
    hook: document::Hook,
    conversion: document::Conversion,
    bot: u64,
    has_viewer: bool,
    has_graphql: bool,
}
fn installation() -> document::Installation {
    document::Installation {
        id: 0,
        login: Box::from([]),
        account: 0,
        kind: document::Account::User,
        created: Wall::EPOCH,
        suspended: false,
    }
}
fn reachable() -> document::Reachable {
    document::Reachable { id: 0, owner: 0, login: Box::from([]), name: Box::from([]), private: false }
}
impl State {
    pub(super) fn new(kind: Kind) -> Self {
        Self {
            kind,
            installation: installation(),
            minted: document::Minted { token: Secret::new(Box::from([])), expires: Wall::EPOCH },
            reachable: reachable(),
            viewer: document::Viewer { id: 0, login: Box::from([]) },
            graphql: document::GraphqlRefusal { kind: Box::from([]), message: Box::from([]) },
            app: document::App { id: 0, slug: Box::from([]) },
            hook: document::Hook { url: Box::from([]) },
            conversion: document::Conversion {
                id: 0,
                slug: Box::from([]),
                owner: Box::from([]),
                owner_id: 0,
                pem: Secret::new(Box::from([])),
                webhook_secret: Secret::new(Box::from([])),
            },
            bot: 0,
            has_viewer: false,
            has_graphql: false,
        }
    }
    pub(super) fn end(&mut self, node: Node) -> Result<Option<Row>, Malformed> {
        match node {
            Node::Installation => {
                if self.kind == Kind::InstallationRows {
                    return Ok(Some(Row::Installation(core::mem::replace(&mut self.installation, installation()))));
                }
            }
            Node::Reachable => return Ok(Some(Row::Reachable(core::mem::replace(&mut self.reachable, reachable())))),
            Node::Viewer => self.has_viewer = true,
            Node::GraphqlError => self.has_graphql = true,
            Node::GraphqlErrors => {
                if !self.has_graphql {
                    return Err(Malformed::Missing { field: b"type" });
                }
            }
            Node::InstallationAccount
            | Node::InstallationRows
            | Node::Minted
            | Node::ReachableEnvelope
            | Node::ReachableRows
            | Node::AppOwner
            | Node::ViewerEnvelope
            | Node::ViewerData
            | Node::App
            | Node::Hook
            | Node::Conversion
            | Node::BotUser => {}
            Node::User
            | Node::RemarkRows
            | Node::Remark
            | Node::RemarkUser
            | Node::CollaboratorRows
            | Node::Collaborator
            | Node::Permission
            | Node::Repository
            | Node::Owner
            | Node::Branch
            | Node::Commit
            | Node::BranchRows
            | Node::BranchName
            | Node::Error
            | Node::Errors
            | Node::IssueRows
            | Node::Issue
            | Node::IssueUser
            | Node::Labels
            | Node::Label
            | Node::PullRequest
            | Node::CommentRows
            | Node::Comment
            | Node::CommentUser
            | Node::PullRows
            | Node::PullObject
            | Node::PullRef { .. }
            | Node::Reviewers
            | Node::Reviewer
            | Node::CombinedObject
            | Node::Contexts
            | Node::Context
            | Node::RunsObject
            | Node::Runs
            | Node::RunItem
            | Node::ErrorEntry => unreachable!("App container dispatch"),
        }
        Ok(None)
    }
    pub(super) fn document(&self) -> Result<Document, Malformed> {
        Ok(match self.kind {
            Kind::Installation => Document::Installation(self.installation.clone()),
            Kind::InstallationRows | Kind::ReachableRows => Document::Rows,
            Kind::Minted => Document::Minted(self.minted.clone()),
            Kind::Viewer => {
                if self.has_graphql {
                    Document::Graphql(self.graphql.clone())
                } else if self.has_viewer {
                    Document::Viewer(self.viewer.clone())
                } else {
                    return Err(Malformed::Missing { field: b"viewer" });
                }
            }
            Kind::App => Document::App(self.app.clone()),
            Kind::HookConfig => Document::HookConfig(self.hook.clone()),
            Kind::Conversion => Document::Conversion(self.conversion.clone()),
            Kind::BotUser => Document::BotUser(self.bot),
            Kind::User
            | Kind::RemarkRows
            | Kind::CollaboratorRows
            | Kind::Permission
            | Kind::Repository
            | Kind::Branch
            | Kind::BranchRows
            | Kind::IssueRows
            | Kind::Issue
            | Kind::CommentRows
            | Kind::Pull
            | Kind::PullRows
            | Kind::CombinedStatus
            | Kind::CheckRuns
            | Kind::Error => unreachable!("App document dispatch"),
        })
    }
    #[expect(
        clippy::too_many_lines,
        reason = "the field dispatcher includes every document node in its unreachable arm"
    )]
    pub(super) fn scalar(&mut self, node: Node, key: &'static [u8], token: Token) -> Result<(), Malformed> {
        match (node, key, token) {
            (Node::Installation, b"id", Token::Number(text)) => self.installation.id = unsigned(&text),
            (Node::Installation, b"created_at", Token::String(text)) => self.installation.created = time(&text, key)?,
            (Node::Installation, b"suspended_at", Token::True) => self.installation.suspended = true,
            (Node::InstallationAccount, b"login", Token::String(text)) => self.installation.login = text,
            (Node::InstallationAccount, b"id", Token::Number(text)) => self.installation.account = unsigned(&text),
            (Node::InstallationAccount, b"type", Token::String(text)) => {
                self.installation.kind = match text.as_ref() {
                    b"User" => document::Account::User,
                    b"Organization" => document::Account::Organization,
                    _ => return Err(Malformed::Value { field: key }),
                };
            }
            (Node::Minted, b"token", Token::String(text)) => self.minted.token = Secret::new(text),
            (Node::Minted, b"expires_at", Token::String(text)) => self.minted.expires = time(&text, key)?,
            (Node::Reachable, b"id", Token::Number(text)) => self.reachable.id = unsigned(&text),
            (Node::Reachable, b"name", Token::String(text)) => self.reachable.name = text,
            (Node::Reachable, b"private", token) => self.reachable.private = token == Token::True,
            (Node::AppOwner, b"id", Token::Number(text)) => {
                if self.kind == Kind::Conversion {
                    self.conversion.owner_id = unsigned(&text);
                } else {
                    self.reachable.owner = unsigned(&text);
                }
            }
            (Node::AppOwner, b"login", Token::String(text)) => {
                if self.kind == Kind::Conversion {
                    self.conversion.owner = text;
                } else {
                    self.reachable.login = text;
                }
            }
            (Node::Viewer, b"databaseId", Token::Number(text)) => self.viewer.id = unsigned(&text),
            (Node::Viewer, b"login", Token::String(text)) => self.viewer.login = text,
            (Node::GraphqlError, b"type", Token::String(text)) => self.graphql.kind = text,
            (Node::GraphqlError, b"message", Token::String(text)) => self.graphql.message = text,
            (Node::App, b"id", Token::Number(text)) => self.app.id = unsigned(&text),
            (Node::App, b"slug", Token::String(text)) => self.app.slug = text,
            (Node::Hook, b"url", Token::String(text)) => self.hook.url = text,
            (Node::Conversion, b"id", Token::Number(text)) => self.conversion.id = unsigned(&text),
            (Node::Conversion, b"slug", Token::String(text)) => self.conversion.slug = text,
            (Node::Conversion, b"pem", Token::String(text)) => self.conversion.pem = Secret::new(text),
            (Node::Conversion, b"webhook_secret", Token::String(text)) => {
                self.conversion.webhook_secret = Secret::new(text);
            }
            (Node::BotUser, b"id", Token::Number(text)) => self.bot = unsigned(&text),
            (
                Node::Installation
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
                | Node::BotUser
                | Node::User
                | Node::RemarkRows
                | Node::Remark
                | Node::RemarkUser
                | Node::CollaboratorRows
                | Node::Collaborator
                | Node::Permission
                | Node::Repository
                | Node::Owner
                | Node::Branch
                | Node::Commit
                | Node::BranchRows
                | Node::BranchName
                | Node::Error
                | Node::Errors
                | Node::IssueRows
                | Node::Issue
                | Node::IssueUser
                | Node::Labels
                | Node::Label
                | Node::PullRequest
                | Node::CommentRows
                | Node::Comment
                | Node::CommentUser
                | Node::PullRows
                | Node::PullObject
                | Node::PullRef { .. }
                | Node::Reviewers
                | Node::Reviewer
                | Node::CombinedObject
                | Node::Contexts
                | Node::Context
                | Node::RunsObject
                | Node::Runs
                | Node::RunItem
                | Node::ErrorEntry,
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
            ) => unreachable!("App field table"),
        }
        Ok(())
    }
}
fn unsigned(text: &[u8]) -> u64 {
    crate::forgejo::id::unsigned(text).expect("walker checked integer")
}
fn time(text: &[u8], field: &'static [u8]) -> Result<Wall, Malformed> {
    crate::forgejo::time::read(text).ok_or(Malformed::Time { field })
}
