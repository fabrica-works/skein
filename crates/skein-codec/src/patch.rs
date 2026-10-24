//! Plan and run bounded family patches (codec.md, section 11).

use crate::schema::{self, DeclIx, FieldIx, Schema, SchemaError, Span, VariantIx};
use skein_lib::List;

mod plan;
mod run;
pub use run::{Build, Halt, Old, Step};
mod text;

#[cfg(test)]
mod tests;

/// The patch text language this machine reads.
pub const LANGUAGE: u16 = 1;

/// The machine's kept texts, plans and per-record limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Capacity {
    /// The most kept schema versions.
    pub versions: u32,
    /// The most plans, including identities.
    pub patches: u32,
    /// The most bytes in a schema or patch text, at most [`schema::MAX_TEXT`].
    pub text: u32,
    /// The most bytes in an old or new record.
    pub bytes: u32,
    /// The most nodes in an old or new record.
    pub nodes: u32,
}

/// A family's kept schemas and planned conversions.
#[derive(Debug)]
pub struct Patcher {
    old: Old,
    build: Build,
}

/// The tables shared by planning and the run's build half (codec.md, 11.6).
#[derive(Debug)]
pub(crate) struct Tables {
    pub(crate) capacity: Capacity,
    pub(crate) schemas: List<Schema>,
    pub(crate) plans: List<plan::Plan>,
}

/// An index made by this patcher for one of its plans.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanIx(u32);

impl PlanIx {
    /// The index counted from zero.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// The kept versions a plan converts between.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Versions {
    /// The old version.
    pub from: u16,
    /// The new version.
    pub to: u16,
}

/// A written site, numbered across its patch's whole text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Site {
    patch: u32,
    index: u32,
}
impl Site {
    /// Names a patch's written site.
    #[must_use]
    pub const fn new(patch: u32, index: u32) -> Site {
        Site { patch, index }
    }
    /// The owner's level of the patch.
    #[must_use]
    pub const fn patch(self) -> u32 {
        self.patch
    }
    /// The site's index in text order.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}

/// A declared refusal, numbered across the patch text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefusalIx(u32);
impl RefusalIx {
    /// Names a refusal in text order.
    #[must_use]
    pub const fn new(index: u32) -> RefusalIx {
        RefusalIx(index)
    }
    /// The refusal's index counted from zero.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Why a schema could not be kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeepError {
    /// The schema text was refused.
    Schema(SchemaError),
    /// The first kept text names another family.
    Family,
    /// This version is already kept.
    Twice,
    /// The text or version table is full.
    Room,
}

/// A patch refusal naming its line and token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PatchError {
    /// The token's line, counted from one.
    pub line: u32,
    /// The token's span in the patch text.
    pub at: Span,
    /// Why the patch was refused.
    pub wrong: PatchWrong,
}

/// A refusal of patch text or its planned changes (codec.md, 11.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PatchWrong {
    /// The text exceeds its capacity.
    Long,
    /// A byte is outside the token alphabet.
    Character,
    /// The grammar wanted another token.
    Expected(Expected),
    /// A decimal exceeds its width.
    Number,
    /// The patch language is unsupported.
    Language,
    /// A block, operation, site, refusal or plan is repeated.
    Twice,
    /// A version has not been kept.
    Unkept,
    /// The second version does not follow the first.
    Version,
    /// A name is absent from its schema.
    Unknown,
    /// The line does not describe the two layouts.
    Mismatch,
    /// A change lacks its operation or written site.
    Missing(Missing),
    /// An added value is still asked of its author.
    Asked,
    /// A value does not fit its type.
    Value,
    /// A written site or value has no permitted target.
    Site,
    /// A refusal does not follow a site or refusal.
    Stray,
    /// A pieces item needs conversion.
    Pieces,
    /// The plans table is full.
    Room,
}

/// What the patch grammar wanted next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Expected {
    /// The language header.
    Language,
    /// The owner header.
    Owner,
    /// A decimal owner or upstream level.
    Level,
    /// A family word.
    Family,
    /// A decimal family version.
    Version,
    /// The word `to`.
    To,
    /// An operation, site, refusal or block.
    Line,
    /// A declaration or part name.
    Name,
    /// A schema type.
    Type,
    /// An added field's value.
    Value,
    /// A quoted refusal text.
    Text,
    /// A payload's record name or `-`.
    Record,
    /// The end of the statement.
    End,
}

/// An operation absent from the patch's family block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Missing {
    /// The operation the change requires.
    pub op: Op,
    /// Its new target, or old target for a drop.
    pub target: Target,
}

/// Changes requiring a line or a written site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    /// Adds a declaration or part.
    Add,
    /// Drops a declaration or part.
    Drop,
    /// Changes record to enumeration or back.
    Reshape,
    /// Adds the version prefix.
    Versioned,
    /// Removes the version prefix.
    Unversioned,
    /// Widens a type.
    Widen,
    /// Adds option levels.
    Wrap,
    /// Changes a type otherwise.
    Retype,
    /// Reorders matched parts.
    Order,
    /// Changes a variant's record.
    Payload,
    /// Supplies a written conversion.
    Written,
}

/// An index in a plan's old or new schema as its operation specifies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// A whole declaration.
    Declaration(DeclIx),
    /// A record's field.
    Field(FieldIx),
    /// An enumeration's variant.
    Variant(VariantIx),
}

/// The header's spans, read again against the same patch text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Header {
    owner: Span,
    level: u32,
    after: Span,
    marks: Span,
}

/// Reads only the patch, owner and mark statements, allocating nothing.
pub fn header(patch: &[u8]) -> Result<Header, PatchError> {
    text::header(patch)
}

impl Header {
    /// The owner's family word in the patch.
    #[must_use]
    pub fn owner(&self) -> Span {
        self.owner
    }
    /// The owner's level after this patch.
    #[must_use]
    pub fn level(&self) -> u32 {
        self.level
    }
    /// The upstream's recorded level, reading this header's text again.
    #[must_use]
    pub fn after(&self, patch: &[u8], upstream: &[u8]) -> Option<u32> {
        text::after(patch, self.after, upstream)
    }
    /// Whether the header declares this mark.
    #[must_use]
    pub fn marked(&self, patch: &[u8], mark: &[u8]) -> bool {
        text::marked(patch, self.marks, mark)
    }
}

impl Patcher {
    /// Allocates the schema and plan tables at their capacities.
    #[must_use]
    pub fn new(capacity: Capacity) -> Patcher {
        assert!(capacity.text <= schema::MAX_TEXT, "patch texts stay within the schema text ceiling");
        Patcher { old: Old::new(capacity.nodes), build: Build::new(capacity) }
    }

    /// Parses and keeps one version of the machine's family.
    pub fn keep(&mut self, text: &[u8]) -> Result<u16, KeepError> {
        if text.len() > usize::try_from(self.build.tables.capacity.text).expect("u32 fits usize")
            || self.build.tables.schemas.room() == 0
        {
            return Err(KeepError::Room);
        }
        let schema = match schema::parse(text) {
            Ok(schema) => schema,
            Err(error) => return Err(KeepError::Schema(error)),
        };
        if let Some(first) = self.build.tables.schemas.get(0)
            && first.spelled(first.family()) != schema.spelled(schema.family())
        {
            return Err(KeepError::Family);
        }
        let version = schema.version();
        if self.schema(version).is_some() {
            return Err(KeepError::Twice);
        }
        self.build.tables.schemas.push(schema).expect("schema room checked");
        Ok(version)
    }

    /// The kept schema of this version, when present.
    #[must_use]
    pub fn schema(&self, version: u16) -> Option<&Schema> {
        self.build.tables.schemas.get(self.build.tables.schema_index(version)?)
    }

    /// Reads the whole text and plans this family's block, if present.
    pub fn plan(&mut self, patch: &[u8]) -> Result<Option<PlanIx>, PatchError> {
        if patch.len() > usize::try_from(self.build.tables.capacity.text).expect("u32 fits usize") {
            return Err(error(1, Span { start: 0, end: 0 }, PatchWrong::Long));
        }
        let parsed = text::parse(patch)?;
        let Some(first) = self.build.tables.schemas.get(0) else {
            return Ok(None);
        };
        for block in &parsed.blocks {
            if spelled(patch, block.family) == first.spelled(first.family()) {
                if self.plan_from(block.from).is_some() {
                    return Err(block.error(PatchWrong::Twice));
                }
                if self.build.tables.plans.room() == 0 {
                    return Err(block.error(PatchWrong::Room));
                }
                let planned = plan::build(&self.build.tables, patch, &parsed, *block)?;
                let index = PlanIx(self.build.tables.plans.len());
                self.build.tables.plans.push(planned).expect("plan room checked");
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    /// Plans a kept version to itself, stopping only at held records.
    pub fn identity(&mut self, version: u16) -> Result<PlanIx, PatchWrong> {
        let schema = self.build.tables.schema_index(version).ok_or(PatchWrong::Unkept)?;
        for plan in &self.build.tables.plans {
            if plan.from == version && plan.from == plan.to {
                return Err(PatchWrong::Twice);
            }
        }
        if self.build.tables.plans.room() == 0 {
            return Err(PatchWrong::Room);
        }
        let planned = plan::identity(&self.build.tables, schema)?;
        let index = PlanIx(self.build.tables.plans.len());
        self.build.tables.plans.push(planned).expect("plan room checked");
        Ok(index)
    }

    /// Finds the patch from a version, excluding an identity plan.
    #[must_use]
    pub fn plan_from(&self, version: u16) -> Option<PlanIx> {
        for (at, plan) in self.build.tables.plans.iter().enumerate() {
            if plan.from == version && plan.from != plan.to {
                return Some(PlanIx(u32::try_from(at).expect("bounded plans table")));
            }
        }
        None
    }

    /// The versions of a plan made by this patcher.
    #[must_use]
    pub fn versions(&self, plan: PlanIx) -> Versions {
        let plan = self.build.tables.plans.get(plan.0).expect("plan belongs to this patcher");
        Versions { from: plan.from, to: plan.to }
    }

    /// An old declaration's new counterpart, when it survives.
    #[must_use]
    pub fn counterpart(&self, plan: PlanIx, old: DeclIx) -> Option<DeclIx> {
        self.build
            .tables
            .plans
            .get(plan.0)
            .expect("plan belongs to this patcher")
            .declarations
            .get(old.index())
            .expect("old declaration belongs to the plan")
            .new
    }

    /// A new declaration's old source, when it has one.
    #[must_use]
    pub fn source(&self, plan: PlanIx, new: DeclIx) -> Option<DeclIx> {
        *self
            .build
            .tables
            .plans
            .get(plan.0)
            .expect("plan belongs to this patcher")
            .olds
            .get(new.index())
            .expect("new declaration belongs to the plan")
    }
}

impl Tables {
    pub(crate) fn schema_index(&self, version: u16) -> Option<u32> {
        for (at, schema) in self.schemas.iter().enumerate() {
            if schema.version() == version {
                return Some(u32::try_from(at).expect("bounded schemas table"));
            }
        }
        None
    }
}

fn error(line: u32, at: Span, wrong: PatchWrong) -> PatchError {
    PatchError { line, at, wrong }
}
fn add(left: u32, right: u32) -> u32 {
    left.checked_add(right).expect("bounded text and schema tables")
}
fn spelled(text: &[u8], span: Span) -> &[u8] {
    let start = usize::try_from(span.start).expect("text offset fits usize");
    let end = usize::try_from(span.end).expect("text offset fits usize");
    text.get(start..end).expect("span belongs to its text")
}
