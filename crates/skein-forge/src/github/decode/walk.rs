//! GitHub's bounded field walker, independent of Forgejo's tables.
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
    Repository,
    Owner,
    Branch,
    Commit,
    BranchRows,
    BranchName,
    Error,
    Errors,
    ErrorEntry,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Read {
    Text,
    Body,
    Number,
    Boolean,
    Container(Node),
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
fn fields(node: Node) -> &'static [Field] {
    match node {
        Node::Repository => {
            const {
                &[
                    field(b"id", Read::Number),
                    field(b"owner", Read::Container(Node::Owner)),
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
        Node::Branch => const { &[field(b"name", Read::Text), field(b"commit", Read::Container(Node::Commit))] },
        Node::Commit => const { &[field(b"sha", Read::Text)] },
        Node::BranchName => const { &[field(b"name", Read::Text)] },
        Node::Error => {
            const {
                &[
                    field(b"message", Read::Body),
                    Field { key: b"errors", read: Read::Container(Node::Errors), required: false },
                ]
            }
        }
        Node::ErrorEntry => const { &[field(b"message", Read::Text)] },
        Node::BranchRows | Node::Errors => &[],
    }
}
fn element(node: Node) -> Option<Node> {
    match node {
        Node::BranchRows => Some(Node::BranchName),
        Node::Errors => Some(Node::ErrorEntry),
        Node::Repository
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
    node: Node,
    touched: u32,
    seen: u32,
}
#[derive(Debug)]
enum Want {
    Start(Node),
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
            document::Kind::Repository => Node::Repository,
            document::Kind::Branch => Node::Branch,
            document::Kind::BranchRows => Node::BranchRows,
            document::Kind::Error => Node::Error,
        };
        Self { open: Stack::with_capacity(DEPTH), want: Want::Start(node), limits: *limits }
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
                    self.start(element(self.open.top().expect("array").node).expect("array role"), &event)
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
    fn start(&mut self, node: Node, event: &Event) -> Result<Move, Malformed> {
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
                            Read::Text => Request::Text(self.limits.string_bytes),
                            Read::Body => Request::Text(self.limits.body_bytes.min(self.limits.string_bytes)),
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
            Read::Text | Read::Body | Read::Number | Read::Boolean => {}
        }
        let wrong = Malformed::Shape { field: field.key };
        let token = match event {
            Event::Token(token) => token,
            Event::Long(_) => return Err(Malformed::TooLong { field: field.key }),
            Event::Skipped(_) | Event::Done | Event::Failed(_) | Event::Closed => return Err(wrong),
        };
        let valid = match (&field.read, &token) {
            (Read::Text | Read::Body, Token::String(text)) => {
                if text.len() > usize::try_from(self.limits.string_bytes).expect("u32 fits usize") {
                    return Err(Malformed::TooLong { field: field.key });
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
                Read::Text | Read::Body | Read::Number | Read::Boolean | Read::Container(_),
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
        Ok(Move::Scalar(node, field.key, token))
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
            self.want = Want::Key;
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
        Ok(Move::End(frame.node))
    }
}
