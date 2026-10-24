//! Preorder conversion and checked written values (codec.md, 11.4–11.7).

use super::{Capacity, Patcher, PlanIx, RefusalIx, Site, Tables, add, plan, spelled};
use crate::Reason;
use crate::schema::{self, DeclIx, FieldIx, Held, Schema, Shape, Span, Type, TypeIx, VariantIx};
use crate::value::{Items, Node, Part, Path, Problem, Refusal, Source, Value};
use alloc::boxed::Box;
use skein_lib::{Buffer, List, Stack};

mod build;
mod heap;
#[cfg(test)]
mod tests;
mod walk;

/// The next place whose value belongs to the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    /// A patch's written conversion, over an old declaration or record.
    Written { site: Site, at: Node },
    /// Bytes holding another family's record; names belong to the old text.
    Held { at: Node, family: Span, name: Span },
    /// The new record is complete.
    Done,
}

/// A record's terminal refusal (codec.md, 11.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Halt {
    /// The old record does not decode.
    Decode(Refusal),
    /// The written site declared this refusal.
    Refused(RefusalIx),
    /// The written site did not declare this refusal.
    Undeclared(RefusalIx),
    /// An old node was read as another kind.
    Misread(Node),
    /// A building operation does not fit the expected new type.
    Misbuilt(Path),
    /// A value fails the new schema's checks.
    Built(Problem),
    /// The bytes, nodes or nesting exceed the capacity.
    Room,
    /// A call is out of turn.
    Order,
    /// The driver's dispatch has no function for this site.
    NoSite(Site),
}

/// The decoded old record, borrowed separately from its builder.
#[derive(Debug)]
pub struct Old {
    value: Value,
    root: Option<Node>,
}

/// Kept plans and the bounded preorder output and cursors.
#[derive(Debug)]
pub struct Build {
    pub(super) tables: Tables,
    out: Buffer,
    walk: Stack<Frame>,
    opened: Stack<Open>,
    state: State,
    plan: Option<PlanIx>,
    pending: Option<Expected>,
    stop: Option<Stop>,
    nodes: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Walking,
    Stopped,
    Done,
    Finished,
}

#[derive(Clone, Copy, Debug)]
struct Expected {
    source: Source,
    path: Path,
}

#[derive(Clone, Copy, Debug)]
struct Stop {
    at: Node,
    site: Option<u32>,
    held: bool,
    declaration: bool,
}

#[derive(Clone, Copy, Debug)]
enum Frame {
    Enter { at: Node, new: Expected, ignore_site: bool },
    Record { at: Node, decl: DeclIx, next: u32, end: u32 },
    List { items: Items, item: TypeIx, path: Path },
}

#[derive(Clone, Copy, Debug)]
struct Open {
    kind: OpenKind,
    path: Path,
    progress: u32,
}
#[derive(Clone, Copy, Debug)]
enum OpenKind {
    Record { decl: DeclIx, first: u32, count: u32 },
    List { item: TypeIx, bound: u32, at: u32 },
    Option { item: TypeIx },
}

impl Old {
    pub(super) fn new(nodes: u32) -> Old {
        Old { value: Value::with_room(nodes), root: None }
    }
    /// The root returned by decoding the running record.
    #[must_use]
    pub fn root(&self) -> Node {
        self.root.expect("a running record has a decoded root")
    }
    /// The node holding this node, or none for the root.
    pub fn parent(&self, node: Node) -> Result<Option<Node>, Halt> {
        match self.value.parent(node) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// A record's field, in the old schema.
    pub fn field(&self, record: Node, field: FieldIx) -> Result<Node, Halt> {
        match self.value.field(record, field) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// A cursor over a list's items.
    pub fn items(&self, list: Node) -> Result<Items, Halt> {
        match self.value.items(list) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// The cursor's next item.
    pub fn next_item(&self, items: &mut Items) -> Option<Node> {
        self.value.next_item(items)
    }
    /// The list's item count.
    pub fn count(&self, list: Node) -> Result<u32, Halt> {
        match self.value.count(list) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// An option's value, if present.
    pub fn some(&self, option: Node) -> Result<Option<Node>, Halt> {
        match self.value.some(option) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// The selected variant in the old schema.
    pub fn tag(&self, enumeration: Node) -> Result<VariantIx, Halt> {
        match self.value.tag(enumeration) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// The selected variant's record, if any.
    pub fn payload(&self, enumeration: Node) -> Result<Option<Node>, Halt> {
        match self.value.payload(enumeration) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// A uint or duration's nanoseconds.
    pub fn uint(&self, node: Node) -> Result<u64, Halt> {
        match self.value.uint(node) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// A boolean.
    pub fn boolean(&self, node: Node) -> Result<bool, Halt> {
        match self.value.boolean(node) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// Fixed, bytes or text content, without its prefix.
    pub fn content(&self, node: Node) -> Result<&[u8], Halt> {
        match self.value.content(node) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    /// A secret's content, without its prefix.
    pub fn expose(&self, node: Node) -> Result<&[u8], Halt> {
        match self.value.expose(node) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
    fn source(&self, node: Node) -> Result<Source, Halt> {
        match self.value.source(node) {
            Ok(value) => Ok(value),
            Err(error) => Err(Halt::Misread(error.node)),
        }
    }
}

impl Patcher {
    /// Decodes a record by the plan's old text and starts its conversion.
    pub fn start(&mut self, plan: PlanIx, root: DeclIx, record: Box<[u8]>) -> Result<(), Halt> {
        self.build.state = State::Idle;
        self.build.plan = Some(plan);
        let result = self.start_record(root, record);
        self.build.settle(result)
    }
    fn start_record(&mut self, root: DeclIx, record: Box<[u8]>) -> Result<(), Halt> {
        if record.len() > usize::try_from(self.build.tables.capacity.bytes).expect("u32 fits usize") {
            return Err(Halt::Room);
        }
        let plan = self.build.current();
        let old = self.build.tables.schemas.get(plan.old).expect("kept old schema");
        let node = match self.old.value.decode(old, root, record) {
            Ok(node) => node,
            Err(refusal) => return Err(Halt::Decode(refusal)),
        };
        self.old.root = Some(node);
        let target = plan.declarations.get(root.index()).expect("old root conversion").new;
        let new = target.ok_or(Halt::Misbuilt(path(old, root)))?;
        let expected = Expected { source: Source::Declaration(new), path: path(self.build.new_schema(), new) };
        self.build.out.clear();
        self.build.clear_stacks();
        self.build.nodes = 0;
        self.build.pending = None;
        self.build.stop = None;
        self.build.push(Frame::Enter { at: node, new: expected, ignore_site: false })?;
        self.build.state = State::Walking;
        Ok(())
    }
    /// Walks at most `3 * capacity.nodes + 1` frames, stopping for one value.
    #[expect(clippy::should_implement_trait, reason = "codec.md, 11.6 names this stopped machine entry point next")]
    pub fn next(&mut self) -> Result<Step, Halt> {
        let result = self.build.advance(&self.old);
        if result.is_err() {
            self.build.state = State::Idle;
        }
        result
    }
    /// Borrows the old record and its builder independently.
    pub fn parts(&mut self) -> (&Old, &mut Build) {
        (&self.old, &mut self.build)
    }
    /// The completed bytes, kept until the next start.
    pub fn finish(&mut self) -> Result<&[u8], Halt> {
        match self.build.state {
            State::Done | State::Finished => {
                self.build.state = State::Finished;
                Ok(self.build.out.as_slice())
            }
            State::Idle | State::Walking | State::Stopped => {
                self.build.state = State::Idle;
                Err(Halt::Order)
            }
        }
    }
    /// Starts another plan over a single exact copy of the finished bytes.
    pub fn next_pass(&mut self, plan: PlanIx, root: DeclIx) -> Result<(), Halt> {
        if self.build.state != State::Finished {
            self.build.state = State::Idle;
            return Err(Halt::Order);
        }
        let record = self.build.out.to_boxed();
        self.start(plan, root, record)
    }
    /// Spells a held name in the last started plan's old schema.
    #[must_use]
    pub fn spelled(&self, span: Span) -> &[u8] {
        let plan = self.build.current();
        self.build.tables.schemas.get(plan.old).expect("kept old schema").spelled(span)
    }
    /// A refusal's unquoted text in the last started plan.
    #[must_use]
    pub fn refusal(&self, refusal: RefusalIx) -> Option<&[u8]> {
        let plan = self.build.tables.plans.get(self.build.plan?.index())?;
        Some(spelled(&plan.text, plan.refusals.get(refusal.index())?.text))
    }
}

impl Build {
    pub(super) fn new(capacity: Capacity) -> Build {
        Build {
            tables: Tables {
                capacity,
                schemas: List::with_capacity(capacity.versions),
                plans: List::with_capacity(capacity.patches),
            },
            out: Buffer::with_capacity(capacity.bytes),
            walk: Stack::with_capacity(capacity.nodes),
            opened: Stack::with_capacity(capacity.nodes),
            state: State::Idle,
            plan: None,
            pending: None,
            stop: None,
            nodes: 0,
        }
    }
    fn clear_stacks(&mut self) {
        for _ in 0..self.walk.len() {
            self.walk.pop().expect("walk has a frame");
        }
        for _ in 0..self.opened.len() {
            self.opened.pop().expect("build has a frame");
        }
    }
    fn current(&self) -> &plan::Plan {
        self.tables.plans.get(self.plan.expect("a run has a plan").index()).expect("plan belongs to this patcher")
    }
    fn new_schema(&self) -> &Schema {
        self.tables.schemas.get(self.current().new).expect("kept new schema")
    }
    fn old_schema(&self) -> &Schema {
        self.tables.schemas.get(self.current().old).expect("kept old schema")
    }
    fn settle(&mut self, result: Result<(), Halt>) -> Result<(), Halt> {
        if result.is_err() {
            self.state = State::Idle;
        }
        result
    }
    fn put(&mut self, bytes: &[u8]) -> Result<(), Halt> {
        match self.out.put(bytes) {
            Ok(()) => Ok(()),
            Err(_) => Err(Halt::Room),
        }
    }
    fn push(&mut self, frame: Frame) -> Result<(), Halt> {
        match self.walk.push(frame) {
            Ok(()) => Ok(()),
            Err(_) => Err(Halt::Room),
        }
    }
    fn count_nodes(&mut self, count: u32) -> Result<(), Halt> {
        self.nodes = self.nodes.checked_add(count).ok_or(Halt::Room)?;
        if self.nodes > self.tables.capacity.nodes {
            return Err(Halt::Room);
        }
        Ok(())
    }
    fn turns(&self) -> u64 {
        u64::from(self.tables.capacity.nodes)
            .checked_mul(3)
            .expect("three u32 fits u64")
            .checked_add(1)
            .expect("three u32 plus one fits u64")
    }
}

fn ty(schema: &Schema, source: Source) -> Type {
    match source {
        Source::Declaration(decl) => Type::Named(decl),
        Source::Type(index) => schema.ty(index),
    }
}
fn path(schema: &Schema, decl: DeclIx) -> Path {
    match schema.declaration(decl).shape {
        Shape::Record { .. } => Path { decl, part: Part::Tail },
        Shape::Enum { .. } => Path { decl, part: Part::Tag },
    }
}
fn built(path: Path, reason: Reason) -> Halt {
    Halt::Built(Problem { path, reason })
}
