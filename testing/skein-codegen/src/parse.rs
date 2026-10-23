//! Schema tables converted to the generator tree (codec.md, section 7).

use core::fmt;

use crate::{Declaration, Enumeration, Field, Record, Schema, Type, Variant, check};

/// A schema refusal with its source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl core::error::Error for Error {}

use skein_codec::schema::{self, DeclIx, Expected, FieldIx, SchemaError, Shape, Span, TypeIx, VariantIx, Wrong};

fn word(source: &str, at: Span) -> &str {
    source
        .get(usize::try_from(at.start).expect("offset fits usize")..usize::try_from(at.end).expect("offset fits usize"))
        .expect("a token is ASCII and belongs to the source")
}

fn refusal(source: &str, error: SchemaError) -> Error {
    let found = || word(source, error.at);
    let message = match error.wrong {
        Wrong::Long => "schema longer than 1048576 bytes".into(),
        Wrong::Many => "more than 4096 declarations".into(),
        Wrong::Character => {
            let character = source
                .get(usize::try_from(error.at.start).expect("offset fits usize")..)
                .expect("character belongs to source")
                .chars()
                .next()
                .expect("character at refusal");
            format!("unexpected character {character:?}")
        }
        Wrong::End => "unexpected end of schema".into(),
        Wrong::Expected(expected) => match expected {
            Expected::Family => format!("expected family, found {}", found()),
            Expected::Open => format!("expected {{, found {}", found()),
            Expected::Colon => format!("expected :, found {}", found()),
            Expected::Record => format!("expected record, found {}", found()),
            Expected::Type => "expected a type".into(),
            Expected::Declaration => format!("expected record, versioned record or enum, found {}", found()),
        },
        Wrong::Name => format!("invalid Rust name {}", found()),
        Wrong::Bound => format!("invalid bound {}", found()),
        Wrong::Family => "invalid family name".into(),
        Wrong::Version => format!("invalid family version {}", found()),
        Wrong::Undeclared => format!("type {} used before declaration", found()),
        Wrong::NotRecord => format!("variant {} must hold one record", found()),
        Wrong::NoVariants => "enum has no variants".into(),
        Wrong::ManyVariants => "enum has more than 256 variants".into(),
        Wrong::DuplicateType => format!("duplicate type {}", found()),
        Wrong::DuplicateField => format!("duplicate field {}", found()),
        Wrong::DuplicateVariant => format!("duplicate variant {}", found()),
        Wrong::Overflow => "encoded worst case overflows u64".into(),
        Wrong::Large => format!("encoded worst case for {} exceeds u32", found()),
    };
    Error { line: usize::try_from(error.line).expect("line fits usize"), message }
}

fn spelling(schema: &schema::Schema, at: Span) -> String {
    core::str::from_utf8(schema.spelled(at)).expect("names are ASCII").into()
}

fn named(schema: &schema::Schema, at: DeclIx) -> String {
    spelling(schema, schema.declaration(at).name)
}

fn ty(schema: &schema::Schema, first: TypeIx) -> Type {
    let mut wrappers = Vec::new();
    let mut at = first;
    let mut value = loop {
        match schema.ty(at) {
            schema::Type::List { bound, item } => {
                wrappers.push(Some(bound));
                at = item;
            }
            schema::Type::Option(item) => {
                wrappers.push(None);
                at = item;
            }
            schema::Type::U8 => break Type::U8,
            schema::Type::U16 => break Type::U16,
            schema::Type::U32 => break Type::U32,
            schema::Type::U64 => break Type::U64,
            schema::Type::Bool => break Type::Bool,
            schema::Type::Duration => break Type::Duration,
            schema::Type::Fixed(bound) => break Type::Fixed(bound),
            schema::Type::Bytes { bound, .. } => break Type::Bytes(bound),
            schema::Type::Secret(bound) => break Type::Secret(bound),
            schema::Type::Text(bound) => break Type::Text(bound),
            schema::Type::Named(at) => break Type::Named(named(schema, at)),
        }
    };
    for wrapper in wrappers.into_iter().rev() {
        value = match wrapper {
            Some(bound) => Type::List(bound, Box::new(value)),
            None => Type::Option(Box::new(value)),
        };
    }
    value
}

fn declaration(schema: &schema::Schema, at: DeclIx) -> Declaration {
    let declaration = schema.declaration(at);
    let name = spelling(schema, declaration.name);
    let line = usize::try_from(declaration.line).expect("line fits usize");
    match declaration.shape {
        Shape::Record { versioned, first, count } => {
            let fields = (first.index()..first.index().checked_add(count).expect("field table bounded"))
                .map(|at| {
                    let field = schema.field(FieldIx::new(at));
                    Field {
                        line: usize::try_from(field.line).expect("line fits usize"),
                        name: spelling(schema, field.name),
                        ty: ty(schema, field.ty),
                    }
                })
                .collect();
            Declaration::Record(Record { line, name, versioned, fields })
        }
        Shape::Enum { first, count } => {
            let variants = (first.index()..first.index().checked_add(count).expect("variant table bounded"))
                .map(|at| {
                    let variant = schema.variant(VariantIx::new(at));
                    Variant {
                        line: usize::try_from(variant.line).expect("line fits usize"),
                        name: spelling(schema, variant.name),
                        record: variant.record.map(|at| named(schema, at)),
                        boxed: variant.boxed,
                    }
                })
                .collect();
            Declaration::Enum(Enumeration { line, name, variants })
        }
    }
}

/// Parses and validates one version of a codec family.
pub fn parse(source: &str) -> Result<Schema, Error> {
    let parsed = schema::parse(source.as_bytes()).map_err(|error| refusal(source, error))?;
    let schema = Schema {
        family: spelling(&parsed, parsed.family()),
        version: parsed.version(),
        declarations: (0..parsed.declarations()).map(|at| declaration(&parsed, DeclIx::new(at))).collect(),
    };
    check::check(&schema)?;
    Ok(schema)
}
