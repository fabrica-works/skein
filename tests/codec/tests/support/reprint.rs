//! Renamed schema text and the name pairs shared with diff tests.

use core::fmt::Write;
use skein_codec::schema::{DeclIx, FieldIx, Held, Schema, Shape, Span, Type, TypeIx, VariantIx};

/// A declaration rename and the field or variant renames within it.
#[derive(Debug)]
pub struct Renamed {
    pub old: String,
    pub new: String,
    pub parts: Vec<(String, String)>,
}

fn spelling(schema: &Schema, span: Span) -> &str {
    core::str::from_utf8(schema.spelled(span)).expect("schema identifier")
}

fn declaration_name(schema: &Schema, at: DeclIx) -> String {
    format!("{}R", spelling(schema, schema.declaration(at).name))
}

/// Names in declaration order; parts are fields or variants in table order.
/// Declaration names gain R, and part names gain _r; held foreign names stay.
pub fn renamed(schema: &Schema) -> Vec<Renamed> {
    let mut names = Vec::new();
    for at in 0..schema.declarations() {
        let declaration = schema.declaration(DeclIx::new(at));
        let mut parts = Vec::new();
        match declaration.shape {
            Shape::Record { first, count, .. } => {
                for offset in 0..count {
                    let field = schema.field(FieldIx::new(first.index().checked_add(offset).expect("field index")));
                    let old = spelling(schema, field.name).to_owned();
                    parts.push((old.clone(), format!("{old}_r")));
                }
            }
            Shape::Enum { first, count } => {
                for offset in 0..count {
                    let variant =
                        schema.variant(VariantIx::new(first.index().checked_add(offset).expect("variant index")));
                    let old = spelling(schema, variant.name).to_owned();
                    parts.push((old.clone(), format!("{old}_r")));
                }
            }
        }
        names.push(Renamed {
            old: spelling(schema, declaration.name).to_owned(),
            new: declaration_name(schema, DeclIx::new(at)),
            parts,
        });
    }
    names
}

/// Reprints every declaration on one line, renames all local names and
/// references, drops comments, and reverses boxing on record payloads.
pub fn reprint(schema: &Schema) -> String {
    let mut out = format!("family {} {}\n", spelling(schema, schema.family()), schema.version());
    for at in 0..schema.declarations() {
        let declaration = schema.declaration(DeclIx::new(at));
        let name = declaration_name(schema, DeclIx::new(at));
        match declaration.shape {
            Shape::Record { versioned, first, count } => {
                if versioned {
                    out.push_str("versioned ");
                }
                write!(out, "record {name} {{ ").expect("string write");
                for offset in 0..count {
                    let field = schema.field(FieldIx::new(first.index().checked_add(offset).expect("field index")));
                    write!(out, "{}_r: ", spelling(schema, field.name)).expect("string write");
                    write_type(schema, field.ty, &mut out);
                    out.push_str(", ");
                }
            }
            Shape::Enum { first, count } => {
                write!(out, "enum {name} {{ ").expect("string write");
                for offset in 0..count {
                    let variant =
                        schema.variant(VariantIx::new(first.index().checked_add(offset).expect("variant index")));
                    write!(out, "{}_r", spelling(schema, variant.name)).expect("string write");
                    if let Some(record) = variant.record {
                        let boxed = if variant.boxed { "" } else { "boxed " };
                        write!(out, ": {boxed}{}", declaration_name(schema, record)).expect("string write");
                    }
                    out.push_str(", ");
                }
            }
        }
        out.push_str("}\n");
    }
    out
}

fn write_type(schema: &Schema, mut at: TypeIx, out: &mut String) {
    // A chain contains at most one type per byte of schema text.
    for _ in 0..schema.text().len() {
        match schema.ty(at) {
            Type::U8 => out.push_str("u8"),
            Type::U16 => out.push_str("u16"),
            Type::U32 => out.push_str("u32"),
            Type::U64 => out.push_str("u64"),
            Type::Bool => out.push_str("bool"),
            Type::Duration => out.push_str("duration"),
            Type::Fixed(bound) => write!(out, "fixed {bound}").expect("string write"),
            Type::Secret(bound) => write!(out, "secret {bound}").expect("string write"),
            Type::Text(bound) => write!(out, "text {bound}").expect("string write"),
            Type::Bytes { bound, held } => {
                write!(out, "bytes {bound}").expect("string write");
                match held {
                    Held::Opaque => {}
                    Held::Record { family, name } => {
                        write!(out, " holds {} {}", spelling(schema, family), spelling(schema, name))
                            .expect("string write");
                    }
                    Held::Pieces(target) => {
                        write!(out, " pieces {}", declaration_name(schema, target)).expect("string write");
                    }
                }
            }
            Type::Named(target) => out.push_str(&declaration_name(schema, target)),
            Type::List { bound, item } => {
                write!(out, "list {bound} ").expect("string write");
                at = item;
                continue;
            }
            Type::Option(item) => {
                out.push_str("option ");
                at = item;
                continue;
            }
        }
        break;
    }
}
