//! Bounded schema tables and layout checks (codec.md, section 7).

use alloc::boxed::Box;
use skein_lib::{List, Map, bytes};

mod grammar;
mod layout;
mod tokens;

#[cfg(test)]
mod tests;

/// The most bytes in a schema's text.
pub const MAX_TEXT: u32 = 1_048_576;
/// The most declarations in a schema.
pub const MAX_DECLARATIONS: u32 = 4_096;

/// An index in the schema's declarations, in the text's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeclIx(u32);

impl DeclIx {
    /// Makes an index in the schema's declarations.
    #[must_use]
    pub const fn new(index: u32) -> DeclIx {
        DeclIx(index)
    }

    /// The index counted from zero.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// An index in the schema's fields, in the text's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldIx(u32);

impl FieldIx {
    /// Makes an index in the schema's fields.
    #[must_use]
    pub const fn new(index: u32) -> FieldIx {
        FieldIx(index)
    }

    /// The index counted from zero.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// An index in the schema's variants, in the text's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VariantIx(u32);

impl VariantIx {
    /// Makes an index in the schema's variants.
    #[must_use]
    pub const fn new(index: u32) -> VariantIx {
        VariantIx(index)
    }

    /// The index counted from zero.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// An index in the schema's types, in the text's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeIx(u32);

impl TypeIx {
    /// Makes an index in the schema's types.
    #[must_use]
    pub const fn new(index: u32) -> TypeIx {
        TypeIx(index)
    }

    /// The index counted from zero.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// A range of the schema's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    /// The first byte of the range.
    pub start: u32,
    /// The byte after the range.
    pub end: u32,
}

/// A record or enumeration in declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Declaration {
    /// The declaration's name in the text.
    pub name: Span,
    /// The line of the name token.
    pub line: u32,
    /// The declaration's fields or variants.
    pub shape: Shape,
}

/// An ordered group of fields or variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// A record, optionally with a version prefix.
    Record {
        /// Whether the record starts with the family's version.
        versioned: bool,
        /// The first field in the fields table.
        first: FieldIx,
        /// The number of fields.
        count: u32,
    },
    /// An enumeration whose variants hold at most one earlier record.
    Enum {
        /// The first variant in the variants table.
        first: VariantIx,
        /// The number of variants.
        count: u32,
    },
}

/// A named field of a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Field {
    /// The field's name in the text.
    pub name: Span,
    /// The line of the name token.
    pub line: u32,
    /// The outermost type in the field's chain.
    pub ty: TypeIx,
}

/// A named variant with an optional record payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Variant {
    /// The variant's name in the text.
    pub name: Span,
    /// The line of the name token.
    pub line: u32,
    /// The record held by the variant, if any.
    pub record: Option<DeclIx>,
    /// Whether the record is boxed without changing its wire bytes.
    pub boxed: bool,
}

/// A field's bounded wire type, with its chain outermost first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// A one-byte integer.
    U8,
    /// A two-byte integer.
    U16,
    /// A four-byte integer.
    U32,
    /// An eight-byte integer.
    U64,
    /// A byte, zero or one.
    Bool,
    /// Eight bytes of nanoseconds.
    Duration,
    /// A fixed number of bytes.
    Fixed(u32),
    /// Bounded secret bytes with a length prefix.
    Secret(u32),
    /// Bounded UTF-8 bytes with a length prefix.
    Text(u32),
    /// Bounded bytes, optionally naming what they hold.
    Bytes {
        /// The most bytes after the length.
        bound: u32,
        /// What the bytes hold.
        held: Held,
    },
    /// Bounded items with a count prefix.
    List {
        /// The most items after the count.
        bound: u32,
        /// The next type in the chain.
        item: TypeIx,
    },
    /// A presence byte followed by the next type when present.
    Option(TypeIx),
    /// A declaration declared before this type's record.
    Named(DeclIx),
}

/// What a bytes field holds, without changing its encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Held {
    /// Bytes whose content the schema does not name.
    Opaque,
    /// A record of another family.
    Record {
        /// The held record's family in the text.
        family: Span,
        /// The held record's name in the text.
        name: Span,
    },
    /// Pieces whose items are an earlier declaration of this schema.
    Pieces(DeclIx),
}

/// A schema refusal naming its source token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SchemaError {
    /// The source line of the refusal.
    pub line: u32,
    /// The token named by the refusal.
    pub at: Span,
    /// Why the schema was refused.
    pub wrong: Wrong,
}

/// Why a schema was refused, by characters, grammar or layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wrong {
    /// The text passed its byte ceiling.
    Long,
    /// The declarations passed their ceiling.
    Many,
    /// A byte outside a comment is no token character.
    Character,
    /// The text ended where a token was wanted.
    End,
    /// A different token was wanted.
    Expected(Expected),
    /// A name is no identifier.
    Name,
    /// A bound is no decimal u32.
    Bound,
    /// A family name is no family word.
    Family,
    /// A family version is no decimal u16.
    Version,
    /// A type was used before its declaration.
    Undeclared,
    /// A variant's payload is no record.
    NotRecord,
    /// An enumeration has no variants.
    NoVariants,
    /// An enumeration has more than 256 variants.
    ManyVariants,
    /// A declaration's name was used before.
    DuplicateType,
    /// A field's name was used before in its record.
    DuplicateField,
    /// A variant's name was used before in its enumeration.
    DuplicateVariant,
    /// The encoded worst case overflows u64.
    Overflow,
    /// The declaration's encoded worst case exceeds u32.
    Large,
}

/// The token wanted by the grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Expected {
    /// The family keyword.
    Family,
    /// An opening brace.
    Open,
    /// A colon after a field's name.
    Colon,
    /// The record keyword after versioned.
    Record,
    /// A type word.
    Type,
    /// A record, versioned record or enumeration.
    Declaration,
}

/// A schema's owned text and bounded tables (codec.md, section 7.3).
#[derive(Debug)]
pub struct Schema {
    text: Box<[u8]>,
    family: Span,
    version: u16,
    declarations: List<Declaration>,
    fields: List<Field>,
    variants: List<Variant>,
    types: List<Type>,
    names: Map<Box<[u8]>, DeclIx>,
    minimum: List<u32>,
    maximum: List<u32>,
}

impl Schema {
    /// The schema's copied text.
    #[must_use]
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// The bytes named by a range of the text.
    #[must_use]
    pub fn spelled(&self, span: Span) -> &[u8] {
        spelled(&self.text, span)
    }

    /// The family's name in the text.
    #[must_use]
    pub fn family(&self) -> Span {
        self.family
    }

    /// The family's version.
    #[must_use]
    pub fn version(&self) -> u16 {
        self.version
    }

    /// The number of declarations in text order.
    #[must_use]
    pub fn declarations(&self) -> u32 {
        self.declarations.len()
    }

    /// The declaration at its table index.
    #[must_use]
    pub fn declaration(&self, at: DeclIx) -> Declaration {
        *self.declarations.get(at.0).expect("declaration index belongs to this schema")
    }

    /// The field at its table index.
    #[must_use]
    pub fn field(&self, at: FieldIx) -> Field {
        *self.fields.get(at.0).expect("field index belongs to this schema")
    }

    /// The variant at its table index.
    #[must_use]
    pub fn variant(&self, at: VariantIx) -> Variant {
        *self.variants.get(at.0).expect("variant index belongs to this schema")
    }

    /// The type at its table index.
    #[must_use]
    pub fn ty(&self, at: TypeIx) -> Type {
        *self.types.get(at.0).expect("type index belongs to this schema")
    }

    /// Looks up a declaration by name in the ordered table.
    #[must_use]
    pub fn find(&self, name: &[u8]) -> Option<DeclIx> {
        self.names.get(name).copied()
    }

    /// The fewest encoded bytes of a type (codec.md, section 7.3).
    #[must_use]
    pub fn fewest_bytes(&self, at: TypeIx) -> u32 {
        match self.ty(at) {
            Type::U8 | Type::Bool | Type::Option(_) => 1,
            Type::U16 => 2,
            Type::U32 | Type::Bytes { .. } | Type::Secret(_) | Type::Text(_) | Type::List { .. } => 4,
            Type::U64 | Type::Duration => 8,
            Type::Fixed(bound) => bound,
            Type::Named(at) => *self.minimum.get(at.0).expect("earlier declaration has its minimum"),
        }
    }

    /// The declaration's encoded worst case at its ceilings.
    #[must_use]
    pub fn most_bytes(&self, at: DeclIx) -> u32 {
        *self.maximum.get(at.0).expect("checked declaration has its worst case")
    }
}

/// Copies, parses and checks one version of a codec family.
pub fn parse(text: &[u8]) -> Result<Schema, SchemaError> {
    if text.len() > usize::try_from(MAX_TEXT).expect("text ceiling fits usize") {
        return Err(SchemaError { line: 1, at: Span { start: 0, end: 0 }, wrong: Wrong::Long });
    }
    let (count, final_line) = tokens::scan(text, None)?;
    let mut tokens = List::with_capacity(count);
    tokens::scan(text, Some(&mut tokens))?;
    let parser = grammar::Parser::new(bytes::copy_of(text), tokens, final_line);
    let mut parsed = parser.parse()?;
    layout::check(&mut parsed)?;
    Ok(parsed.schema)
}

fn spelled(text: &[u8], span: Span) -> &[u8] {
    let start = usize::try_from(span.start).expect("text offset fits usize");
    let end = usize::try_from(span.end).expect("text offset fits usize");
    text.get(start..end).expect("span belongs to the schema text")
}

fn add(left: u32, right: u32) -> u32 {
    left.checked_add(right).expect("table indices and lines are bounded by the text ceiling")
}
