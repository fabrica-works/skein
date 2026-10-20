//! GitHub's bounded field walker, independent of Forgejo's tables.
use super::super::delivery::{Ci, Event as Hook};
use super::super::{
    document,
    limits::{DEPTH, Limits},
};
use super::Malformed;
use skein_json::{
    Token,
    tokenizer::{self, Event, Request},
};
use skein_lib::Stack;
/// Container roles in the first GitHub documents.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Node {
    Installation,
    InstallationAccount,
    InstallationRows,
    Minted,
    ReachableEnvelope,
    ReachableRows,
    Reachable,
    AppOwner,
    ViewerEnvelope,
    ViewerData,
    Viewer,
    GraphqlErrors,
    GraphqlError,
    App,
    Hook,
    Conversion,
    BotUser,
    User,
    Permission,
    CollaboratorRows,
    RemarkRows,
    Remark,
    RemarkUser,
    Collaborator,
    Repository,
    Owner,
    Branch,
    Commit,
    BranchRows,
    BranchName,
    Error,
    Errors,
    ErrorEntry,
    IssueRows,
    Issue,
    IssueUser,
    Labels,
    Label,
    PullRequest,
    CommentRows,
    Comment,
    CommentUser,
    PullRows,
    PullObject,
    PullRef { head: bool },
    Reviewers,
    Reviewer,
    CombinedObject,
    Contexts,
    Context,
    RunsObject,
    Runs,
    RunItem,
}
/// Payload roles share the walker without extending the document-only Node vocabulary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Payload {
    Root(Hook),
    Repository,
    Item,
    Number,
    Body,
    Head,
    Sender,
    Installation,
    InstallationId,
    Account,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Document(Node),
    Payload(Payload),
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Read {
    Text,
    Body,
    LongBody,
    Secret,
    Presence,
    Number,
    Boolean,
    Container(Role),
}
#[derive(Clone, Copy, Debug)]
struct Field {
    key: &'static [u8],
    read: Read,
    required: bool,
}
const fn field(key: &'static [u8], read: Read) -> Field {
    Field { key, read, required: true }
}
const fn optional(key: &'static [u8], read: Read) -> Field {
    Field { key, read, required: false }
}
fn issue_fields() -> &'static [Field] {
    const {
        &[
            field(b"number", Read::Number),
            field(b"state", Read::Text),
            field(b"user", Read::Container(Role::Document(Node::IssueUser))),
            optional(b"labels", Read::Container(Role::Document(Node::Labels))),
            field(b"updated_at", Read::Text),
            field(b"title", Read::Text),
            optional(b"body", Read::LongBody),
            optional(b"pull_request", Read::Container(Role::Document(Node::PullRequest))),
        ]
    }
}
fn comment_fields() -> &'static [Field] {
    const {
        &[
            field(b"id", Read::Number),
            field(b"user", Read::Container(Role::Document(Node::CommentUser))),
            optional(b"created_at", Read::Text),
            optional(b"updated_at", Read::Text),
            optional(b"body", Read::LongBody),
        ]
    }
}
#[expect(clippy::too_many_lines, reason = "one exhaustive table lists the fields of every document node")]
fn document_fields(node: Node) -> &'static [Field] {
    match node {
        Node::Remark => {
            const {
                &[
                    field(b"id", Read::Number),
                    field(b"user", Read::Container(Role::Document(Node::RemarkUser))),
                    field(b"path", Read::Text),
                    optional(b"line", Read::Number),
                    optional(b"original_line", Read::Number),
                    optional(b"body", Read::LongBody),
                ]
            }
        }
        Node::Collaborator => {
            const { &[field(b"id", Read::Number), field(b"role_name", Read::Text), optional(b"login", Read::Text)] }
        }
        Node::User => const { &[field(b"id", Read::Number), field(b"login", Read::Text)] },
        Node::Permission => const { &[field(b"role_name", Read::Text)] },
        Node::PullObject => {
            const {
                &[
                    field(b"number", Read::Number),
                    field(b"state", Read::Text),
                    optional(b"merged", Read::Boolean),
                    optional(b"merge_commit_sha", Read::Text),
                    optional(b"mergeable", Read::Boolean),
                    field(b"head", Read::Container(Role::Document(Node::PullRef { head: true }))),
                    field(b"base", Read::Container(Role::Document(Node::PullRef { head: false }))),
                    optional(b"requested_reviewers", Read::Container(Role::Document(Node::Reviewers))),
                ]
            }
        }
        Node::PullRef { .. } => const { &[field(b"ref", Read::Text), field(b"sha", Read::Text)] },
        Node::CombinedObject => {
            const {
                &[
                    field(b"state", Read::Text),
                    field(b"total_count", Read::Number),
                    optional(b"statuses", Read::Container(Role::Document(Node::Contexts))),
                ]
            }
        }
        Node::Context => {
            const {
                &[
                    optional(b"updated_at", Read::Text),
                    field(b"context", Read::Text),
                    field(b"state", Read::Text),
                    optional(b"description", Read::Text),
                    optional(b"target_url", Read::Text),
                ]
            }
        }
        Node::RunsObject => const { &[field(b"check_runs", Read::Container(Role::Document(Node::Runs)))] },
        Node::RunItem => const { &[field(b"status", Read::Text), optional(b"conclusion", Read::Text)] },
        Node::Issue => issue_fields(),
        Node::RemarkUser | Node::Reviewer | Node::IssueUser | Node::CommentUser | Node::BotUser => {
            const { &[field(b"id", Read::Number)] }
        }
        Node::Label | Node::BranchName => const { &[field(b"name", Read::Text)] },
        Node::PullRequest => const { &[optional(b"merged_at", Read::Presence)] },
        Node::Comment => comment_fields(),
        Node::Installation => {
            const {
                &[
                    field(b"id", Read::Number),
                    field(b"account", Read::Container(Role::Document(Node::InstallationAccount))),
                    field(b"created_at", Read::Text),
                    optional(b"suspended_at", Read::Presence),
                ]
            }
        }
        Node::InstallationAccount => {
            const { &[field(b"login", Read::Text), field(b"id", Read::Number), field(b"type", Read::Text)] }
        }
        Node::Minted => const { &[field(b"token", Read::Secret), field(b"expires_at", Read::Text)] },
        Node::ReachableEnvelope => {
            const { &[field(b"repositories", Read::Container(Role::Document(Node::ReachableRows)))] }
        }
        Node::Reachable => {
            const {
                &[
                    field(b"id", Read::Number),
                    field(b"owner", Read::Container(Role::Document(Node::AppOwner))),
                    field(b"name", Read::Text),
                    field(b"private", Read::Boolean),
                ]
            }
        }
        Node::AppOwner => const { &[field(b"login", Read::Text), field(b"id", Read::Number)] },
        Node::ViewerEnvelope => {
            const {
                &[
                    optional(b"data", Read::Container(Role::Document(Node::ViewerData))),
                    optional(b"errors", Read::Container(Role::Document(Node::GraphqlErrors))),
                ]
            }
        }
        Node::ViewerData => const { &[optional(b"viewer", Read::Container(Role::Document(Node::Viewer)))] },
        Node::Viewer => const { &[field(b"databaseId", Read::Number), field(b"login", Read::Text)] },
        Node::GraphqlError => const { &[field(b"type", Read::Text), field(b"message", Read::Text)] },
        Node::App => const { &[field(b"id", Read::Number), field(b"slug", Read::Text)] },
        Node::Hook => const { &[field(b"url", Read::Text)] },
        Node::Conversion => {
            const {
                &[
                    field(b"id", Read::Number),
                    field(b"slug", Read::Text),
                    field(b"owner", Read::Container(Role::Document(Node::AppOwner))),
                    field(b"pem", Read::Secret),
                    field(b"webhook_secret", Read::Secret),
                ]
            }
        }
        Node::Repository => {
            const {
                &[
                    field(b"id", Read::Number),
                    field(b"owner", Read::Container(Role::Document(Node::Owner))),
                    field(b"name", Read::Text),
                    field(b"default_branch", Read::Text),
                    field(b"html_url", Read::Text),
                    field(b"allow_merge_commit", Read::Boolean),
                    field(b"allow_squash_merge", Read::Boolean),
                    field(b"allow_rebase_merge", Read::Boolean),
                ]
            }
        }
        Node::Owner => const { &[field(b"login", Read::Text), field(b"id", Read::Number), field(b"type", Read::Text)] },
        Node::Branch => {
            const { &[field(b"name", Read::Text), field(b"commit", Read::Container(Role::Document(Node::Commit)))] }
        }
        Node::Commit => const { &[field(b"sha", Read::Text)] },
        Node::Error => {
            const {
                &[
                    field(b"message", Read::Body),
                    Field { key: b"errors", read: Read::Container(Role::Document(Node::Errors)), required: false },
                ]
            }
        }
        Node::ErrorEntry => const { &[field(b"message", Read::Text)] },
        Node::RemarkRows
        | Node::CollaboratorRows
        | Node::PullRows
        | Node::Reviewers
        | Node::Contexts
        | Node::Runs
        | Node::IssueRows
        | Node::CommentRows
        | Node::Labels
        | Node::InstallationRows
        | Node::ReachableRows
        | Node::GraphqlErrors
        | Node::BranchRows
        | Node::Errors => &[],
    }
}
fn element(role: Role) -> Option<Role> {
    let node = match role {
        Role::Document(node) => node,
        Role::Payload(_) => return None,
    };
    match node {
        Node::RemarkRows => Some(Role::Document(Node::Remark)),
        Node::CollaboratorRows => Some(Role::Document(Node::Collaborator)),
        Node::PullRows => Some(Role::Document(Node::PullObject)),
        Node::Reviewers => Some(Role::Document(Node::Reviewer)),
        Node::Contexts => Some(Role::Document(Node::Context)),
        Node::Runs => Some(Role::Document(Node::RunItem)),
        Node::IssueRows => Some(Role::Document(Node::Issue)),
        Node::CommentRows => Some(Role::Document(Node::Comment)),
        Node::Labels => Some(Role::Document(Node::Label)),
        Node::InstallationRows => Some(Role::Document(Node::Installation)),
        Node::ReachableRows => Some(Role::Document(Node::Reachable)),
        Node::GraphqlErrors => Some(Role::Document(Node::GraphqlError)),
        Node::BranchRows => Some(Role::Document(Node::BranchName)),
        Node::Errors => Some(Role::Document(Node::ErrorEntry)),
        Node::PullObject
        | Node::PullRef { .. }
        | Node::Reviewer
        | Node::CombinedObject
        | Node::Context
        | Node::RunsObject
        | Node::RunItem
        | Node::Issue
        | Node::IssueUser
        | Node::Label
        | Node::PullRequest
        | Node::Comment
        | Node::CommentUser
        | Node::Installation
        | Node::InstallationAccount
        | Node::Minted
        | Node::ReachableEnvelope
        | Node::Reachable
        | Node::AppOwner
        | Node::ViewerEnvelope
        | Node::ViewerData
        | Node::Viewer
        | Node::GraphqlError
        | Node::App
        | Node::Hook
        | Node::Conversion
        | Node::User
        | Node::Permission
        | Node::Remark
        | Node::RemarkUser
        | Node::Collaborator
        | Node::BotUser
        | Node::Repository
        | Node::Owner
        | Node::Branch
        | Node::Commit
        | Node::BranchName
        | Node::Error
        | Node::ErrorEntry => None,
    }
}
#[derive(Debug)]
struct Frame {
    node: Role,
    touched: u32,
    seen: u32,
}
#[derive(Debug)]
enum Want {
    Start(Role),
    Key,
    Value(Field, u32),
    Element,
    Unknown(u32),
    Done,
    Terminal,
}
/// One field or container completion, with the next tokenizer demand.
#[derive(Debug)]
pub(super) enum Move {
    Demand(Request),
    Scalar(Node, &'static [u8], Token),
    End(Node),
    PayloadScalar(Payload, &'static [u8], Token),
    PayloadEnd,
    BodyLong(Option<Node>, u64),
    Done,
}
/// A fixed-depth walker retaining field masks and no document tree.
#[derive(Debug)]
pub(super) struct Walk {
    open: Stack<Frame>,
    want: Want,
    limits: Limits,
}
impl Walk {
    pub(super) fn new(kind: document::Kind, limits: &Limits) -> Self {
        let node = match kind {
            document::Kind::Installation => Node::Installation,
            document::Kind::InstallationRows => Node::InstallationRows,
            document::Kind::Minted => Node::Minted,
            document::Kind::ReachableRows => Node::ReachableEnvelope,
            document::Kind::Viewer => Node::ViewerEnvelope,
            document::Kind::App => Node::App,
            document::Kind::HookConfig => Node::Hook,
            document::Kind::Conversion => Node::Conversion,
            document::Kind::BotUser => Node::BotUser,
            document::Kind::User => Node::User,
            document::Kind::Permission => Node::Permission,
            document::Kind::RemarkRows => Node::RemarkRows,
            document::Kind::CollaboratorRows => Node::CollaboratorRows,
            document::Kind::Repository => Node::Repository,
            document::Kind::Branch => Node::Branch,
            document::Kind::BranchRows => Node::BranchRows,
            document::Kind::IssueRows => Node::IssueRows,
            document::Kind::Issue => Node::Issue,
            document::Kind::CommentRows => Node::CommentRows,
            document::Kind::Pull => Node::PullObject,
            document::Kind::PullRows => Node::PullRows,
            document::Kind::CombinedStatus => Node::CombinedObject,
            document::Kind::CheckRuns => Node::RunsObject,
            document::Kind::Error => Node::Error,
        };
        Self::root(Role::Document(node), limits)
    }
    /// A delivery has its own root but uses the same field masks and depth bound.
    pub(super) fn delivery(event: Hook, limits: &Limits) -> Self {
        Self::root(Role::Payload(Payload::Root(event)), limits)
    }
    fn root(role: Role, limits: &Limits) -> Self {
        Self { open: Stack::with_capacity(DEPTH), want: Want::Start(role), limits: *limits }
    }
    pub(super) fn step(&mut self, event: Event) -> Result<Move, Malformed> {
        if let Event::Failed(error) = event {
            return Err(Malformed::Json(error));
        }
        let want = core::mem::replace(&mut self.want, Want::Terminal);
        match want {
            Want::Start(node) => self.start(node, &event),
            Want::Key => self.key(event),
            Want::Value(field, bit) => self.value(field, bit, event),
            Want::Element => {
                if event == Event::Token(Token::ArrayEnd) {
                    self.end()
                } else {
                    let frame = self.open.top_mut().expect("array");
                    if frame.node == Role::Document(Node::GraphqlErrors) && frame.seen != 0 {
                        return self.unknown(&event, 0);
                    }
                    frame.seen = 1;
                    let node = element(frame.node).expect("array role");
                    self.start(node, &event)
                }
            }
            Want::Unknown(depth) => self.unknown(&event, depth),
            Want::Done => {
                if event == Event::Done {
                    Ok(Move::Done)
                } else {
                    Err(Malformed::Shape { field: b"" })
                }
            }
            Want::Terminal => Err(Malformed::Shape { field: b"" }),
        }
    }
    fn start(&mut self, node: Role, event: &Event) -> Result<Move, Malformed> {
        let array = element(node).is_some();
        if event != &Event::Token(if array { Token::ArrayStart } else { Token::ObjectStart }) {
            return Err(Malformed::Shape { field: b"" });
        }
        if self.open.push(Frame { node, touched: 0, seen: 0 }).is_err() {
            return Err(Malformed::Json(tokenizer::Error::TooDeep));
        }
        self.want = if array { Want::Element } else { Want::Key };
        Ok(Move::Demand(Request::Next))
    }
    fn key(&mut self, event: Event) -> Result<Move, Malformed> {
        match event {
            Event::Token(Token::ObjectEnd) => self.end(),
            Event::Token(Token::Key(key)) => {
                let frame = self.open.top_mut().expect("object key");
                for (index, field) in fields(frame.node).iter().enumerate() {
                    if field.key == key.as_ref() {
                        let bit = 1u32.checked_shl(u32::try_from(index).expect("small table")).expect("field bit");
                        if frame.touched & bit != 0 {
                            return Err(Malformed::Shape { field: field.key });
                        }
                        frame.touched |= bit;
                        self.want = Want::Value(*field, bit);
                        return Ok(Move::Demand(match field.read {
                            Read::Text | Read::Secret => Request::Text(self.limits.string_bytes),
                            Read::Body => Request::Text(self.limits.body_bytes.min(self.limits.string_bytes)),
                            Read::LongBody => Request::Text(self.limits.body_bytes),
                            Read::Presence => Request::Text(0),
                            Read::Number | Read::Boolean | Read::Container(_) => Request::Next,
                        }));
                    }
                }
                self.want = Want::Unknown(0);
                Ok(Move::Demand(Request::Text(0)))
            }
            Event::Token(
                Token::ObjectStart
                | Token::ArrayStart
                | Token::ArrayEnd
                | Token::String(_)
                | Token::Number(_)
                | Token::True
                | Token::False
                | Token::Null,
            )
            | Event::Long(_)
            | Event::Skipped(_)
            | Event::Done
            | Event::Failed(_)
            | Event::Closed => Err(Malformed::Shape { field: b"" }),
        }
    }
    fn value(&mut self, field: Field, bit: u32, event: Event) -> Result<Move, Malformed> {
        if event == Event::Token(Token::Null) {
            self.want = Want::Key;
            return Ok(Move::Demand(Request::Next));
        }
        let frame = self.open.top_mut().expect("field value");
        frame.seen |= bit;
        let node = frame.node;
        match field.read {
            Read::Container(child) => return self.start(child, &event),
            Read::Presence => {
                self.unknown(&event, 0)?;
                return Ok(scalar(node, field.key, Token::True));
            }
            Read::Text | Read::Secret | Read::Body | Read::LongBody | Read::Number | Read::Boolean => {}
        }
        let wrong = Malformed::Shape { field: field.key };
        let token = match event {
            Event::Token(token) => token,
            Event::Long(length) => {
                if field.read == Read::LongBody {
                    self.want = Want::Key;
                    return Ok(Move::BodyLong(
                        match node {
                            Role::Document(node) => Some(node),
                            Role::Payload(_) => None,
                        },
                        length,
                    ));
                }
                return Err(if field.read == Read::Secret {
                    Malformed::Bound { field: field.key }
                } else {
                    Malformed::TooLong { field: field.key }
                });
            }
            Event::Skipped(_) | Event::Done | Event::Failed(_) | Event::Closed => return Err(wrong),
        };
        let valid = match (&field.read, &token) {
            (Read::Text | Read::Secret | Read::Body | Read::LongBody, Token::String(text)) => {
                if field.read != Read::LongBody
                    && text.len() > usize::try_from(self.limits.string_bytes).expect("u32 fits usize")
                {
                    return Err(if field.read == Read::Secret {
                        Malformed::Bound { field: field.key }
                    } else {
                        Malformed::TooLong { field: field.key }
                    });
                }
                true
            }
            (Read::Number, Token::Number(text)) => {
                if crate::forgejo::id::unsigned(text).is_none() {
                    return Err(Malformed::Range { field: field.key });
                }
                true
            }
            (Read::Boolean, Token::True | Token::False) => true,
            (
                Read::Text
                | Read::Secret
                | Read::Presence
                | Read::Body
                | Read::LongBody
                | Read::Number
                | Read::Boolean
                | Read::Container(_),
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
            ) => false,
        };
        if !valid {
            return Err(wrong);
        }
        self.want = Want::Key;
        Ok(scalar(node, field.key, token))
    }
    fn unknown(&mut self, event: &Event, depth: u32) -> Result<Move, Malformed> {
        let depth = match event {
            Event::Token(Token::ObjectStart | Token::ArrayStart) => {
                depth.checked_add(1).ok_or(Malformed::Json(tokenizer::Error::TooDeep))?
            }
            Event::Token(Token::ObjectEnd | Token::ArrayEnd) => {
                depth.checked_sub(1).ok_or(Malformed::Shape { field: b"" })?
            }
            Event::Token(Token::Key(_)) => {
                self.want = Want::Unknown(depth);
                return Ok(Move::Demand(Request::Text(0)));
            }
            Event::Token(Token::String(_) | Token::Number(_) | Token::True | Token::False | Token::Null)
            | Event::Long(_) => depth,
            Event::Skipped(_) | Event::Done | Event::Failed(_) | Event::Closed => {
                return Err(Malformed::Shape { field: b"" });
            }
        };
        if self.open.len().saturating_add(depth) > DEPTH {
            return Err(Malformed::Json(tokenizer::Error::TooDeep));
        }
        if depth == 0 {
            self.want = if element(self.open.top().expect("unknown parent").node).is_some() {
                Want::Element
            } else {
                Want::Key
            };
            Ok(Move::Demand(Request::Next))
        } else {
            self.want = Want::Unknown(depth);
            Ok(Move::Demand(Request::Text(0)))
        }
    }
    fn end(&mut self) -> Result<Move, Malformed> {
        let frame = self.open.pop().expect("container end");
        for (index, field) in fields(frame.node).iter().enumerate() {
            let bit = 1u32.checked_shl(u32::try_from(index).expect("small table")).expect("field bit");
            if field.required && frame.seen & bit == 0 {
                return Err(Malformed::Missing { field: field.key });
            }
        }
        self.want = match self.open.top() {
            None => Want::Done,
            Some(frame) => {
                if element(frame.node).is_some() {
                    Want::Element
                } else {
                    Want::Key
                }
            }
        };
        Ok(match frame.node {
            Role::Document(node) => Move::End(node),
            Role::Payload(_) => Move::PayloadEnd,
        })
    }
}

fn scalar(role: Role, key: &'static [u8], token: Token) -> Move {
    match role {
        Role::Document(node) => Move::Scalar(node, key, token),
        Role::Payload(node) => Move::PayloadScalar(node, key, token),
    }
}
fn fields(role: Role) -> &'static [Field] {
    match role {
        Role::Document(node) => document_fields(node),
        Role::Payload(node) => payload_fields(node),
    }
}
const fn child(key: &'static [u8], node: Payload) -> Field {
    field(key, Read::Container(Role::Payload(node)))
}
/// The common action/object/repository shape used by item and CI events.
const fn action_fields(key: &'static [u8], node: Payload) -> [Field; 3] {
    [field(b"action", Read::Text), child(key, node), child(b"repository", Payload::Repository)]
}
fn payload_fields(node: Payload) -> &'static [Field] {
    match node {
        Payload::Root(Hook::Issue) => const { &action_fields(b"issue", Payload::Item) },
        Payload::Root(Hook::Comment) => {
            const {
                &[
                    field(b"action", Read::Text),
                    child(b"issue", Payload::Number),
                    child(b"comment", Payload::Body),
                    child(b"repository", Payload::Repository),
                ]
            }
        }
        Payload::Root(Hook::Pull) => {
            const {
                &[
                    field(b"action", Read::Text),
                    field(b"number", Read::Number),
                    optional(b"reason", Read::Text),
                    child(b"repository", Payload::Repository),
                ]
            }
        }
        Payload::Root(Hook::Review) => {
            const {
                &[
                    field(b"action", Read::Text),
                    child(b"pull_request", Payload::Number),
                    child(b"review", Payload::Body),
                    child(b"repository", Payload::Repository),
                ]
            }
        }
        Payload::Root(Hook::Remark) => const { &action_fields(b"pull_request", Payload::Number) },
        Payload::Root(Hook::Push) => {
            const {
                &[
                    field(b"ref", Read::Text),
                    field(b"after", Read::Text),
                    field(b"deleted", Read::Boolean),
                    child(b"repository", Payload::Repository),
                    child(b"sender", Payload::Sender),
                ]
            }
        }
        Payload::Root(Hook::Ref(_)) => {
            const { &[field(b"ref", Read::Text), field(b"ref_type", Read::Text), child(b"repository", Payload::Repository)] }
        }
        Payload::Root(Hook::Ci(event)) => ci_fields(event),
        Payload::Root(Hook::Installation | Hook::Repositories) => {
            const { &[field(b"action", Read::Text), child(b"installation", Payload::Installation)] }
        }
        Payload::Root(Hook::Target) => {
            const {
                &[
                    field(b"action", Read::Text),
                    child(b"installation", Payload::InstallationId),
                    child(b"account", Payload::Account),
                ]
            }
        }
        Payload::Repository | Payload::Sender | Payload::InstallationId => const { &[field(b"id", Read::Number)] },
        Payload::Item => const { &[field(b"number", Read::Number), optional(b"body", Read::LongBody)] },
        Payload::Number => const { &[field(b"number", Read::Number)] },
        Payload::Body => const { &[optional(b"body", Read::LongBody)] },
        Payload::Head => const { &[field(b"head_sha", Read::Text)] },
        // Target puts the account beside the installation, the other two events inside it.
        Payload::Installation => const { &[field(b"id", Read::Number), child(b"account", Payload::Account)] },
        Payload::Account => const { &[field(b"id", Read::Number), field(b"login", Read::Text)] },
    }
}

fn ci_fields(event: Ci) -> &'static [Field] {
    match event {
        Ci::CheckRun => const { &action_fields(b"check_run", Payload::Head) },
        Ci::CheckSuite => const { &action_fields(b"check_suite", Payload::Head) },
        Ci::WorkflowJob => const { &action_fields(b"workflow_job", Payload::Head) },
        Ci::WorkflowRun => const { &action_fields(b"workflow_run", Payload::Head) },
        Ci::ChecksRequested | Ci::Destroyed => const { &action_fields(b"merge_group", Payload::Head) },
        Ci::Status => {
            const { &[field(b"sha", Read::Text), field(b"state", Read::Text), child(b"repository", Payload::Repository)] }
        }
    }
}
