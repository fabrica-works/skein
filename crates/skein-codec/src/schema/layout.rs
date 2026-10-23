//! Resolve earlier declarations and cache their encoded bounds without recursion.

use super::grammar::Parsed;
use super::{
    DeclIx, Declaration, Field, FieldIx, Held, Schema, SchemaError, Shape, Span, Type, TypeIx, VariantIx, Wrong, add,
};
use skein_lib::{Set, bytes};

fn error(line: u32, at: Span, wrong: Wrong) -> SchemaError {
    SchemaError { line, at, wrong }
}

fn sum(left: u64, right: u64, line: u32, at: Span) -> Result<u64, SchemaError> {
    left.checked_add(right).ok_or(error(line, at, Wrong::Overflow))
}

fn resolve(schema: &Schema, name: Span, line: u32) -> Result<DeclIx, SchemaError> {
    schema.find(schema.spelled(name)).ok_or(error(line, name, Wrong::Undeclared))
}

fn field_size(parsed: &mut Parsed, field: Field) -> Result<u64, SchemaError> {
    let mut end = field.ty.index();
    // Every wrapper advances to the next type; the chain ends within the table.
    for _ in 0..parsed.schema.types.len() {
        let ty = parsed.schema.ty(TypeIx::new(end));
        let name = *parsed.references.get(end).expect("one reference span per type");
        match ty {
            Type::List { item, .. } | Type::Option(item) => {
                end = item.index();
            }
            Type::Named(_) => {
                let at = resolve(&parsed.schema, name, field.line)?;
                *parsed.schema.types.get_mut(end).expect("type belongs to this table") = Type::Named(at);
                break;
            }
            Type::Bytes { bound, held: Held::Pieces(_) } => {
                let at = resolve(&parsed.schema, name, field.line)?;
                *parsed.schema.types.get_mut(end).expect("type belongs to this table") =
                    Type::Bytes { bound, held: Held::Pieces(at) };
                break;
            }
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Secret(_)
            | Type::Text(_)
            | Type::Bytes { held: Held::Opaque | Held::Record { .. }, .. } => break,
        }
    }
    let mut size = 0;
    for at in (field.ty.index()..=end).rev() {
        size = match parsed.schema.ty(TypeIx::new(at)) {
            Type::U8 | Type::Bool => 1,
            Type::U16 => 2,
            Type::U32 => 4,
            Type::U64 | Type::Duration => 8,
            Type::Fixed(bound) => u64::from(bound),
            Type::Bytes { bound, .. } | Type::Secret(bound) | Type::Text(bound) => {
                sum(4, u64::from(bound), field.line, field.name)?
            }
            Type::Named(at) => u64::from(parsed.schema.most_bytes(at)),
            Type::Option(_) => sum(1, size, field.line, field.name)?,
            Type::List { bound, .. } => {
                let contents =
                    size.checked_mul(u64::from(bound)).ok_or(error(field.line, field.name, Wrong::Overflow))?;
                sum(4, contents, field.line, field.name)?
            }
        };
    }
    Ok(size)
}

fn record_size(parsed: &mut Parsed, versioned: bool, first: FieldIx, count: u32) -> Result<(u64, u64), SchemaError> {
    let mut names = Set::with_capacity(count);
    let mut maximum = if versioned { 2 } else { 0 };
    let mut minimum = maximum;
    for at in first.index()..add(first.index(), count) {
        let field = parsed.schema.field(FieldIx::new(at));
        let name = bytes::copy_of(parsed.schema.spelled(field.name));
        if !names.insert(name).expect("at most one name per field") {
            return Err(error(field.line, field.name, Wrong::DuplicateField));
        }
        let size = field_size(parsed, field)?;
        maximum = sum(maximum, size, field.line, field.name)?;
        minimum = sum(minimum, u64::from(parsed.schema.fewest_bytes(field.ty)), field.line, field.name)?;
    }
    Ok((minimum, maximum))
}

fn enum_size(
    parsed: &mut Parsed,
    declaration: Declaration,
    first: VariantIx,
    count: u32,
) -> Result<(u64, u64), SchemaError> {
    if count == 0 {
        return Err(error(declaration.line, declaration.name, Wrong::NoVariants));
    }
    if count > 256 {
        return Err(error(declaration.line, declaration.name, Wrong::ManyVariants));
    }
    let mut names = Set::with_capacity(count);
    let mut maximum = 0;
    let mut minimum = u64::MAX;
    for at in first.index()..add(first.index(), count) {
        let variant = parsed.schema.variant(VariantIx::new(at));
        if !names.insert(bytes::copy_of(parsed.schema.spelled(variant.name))).expect("at most one name per variant") {
            return Err(error(variant.line, variant.name, Wrong::DuplicateVariant));
        }
        let (least, most) = match *parsed.payloads.get(at).expect("one payload span per variant") {
            Some(name) => {
                let record = resolve(&parsed.schema, name, variant.line)?;
                match parsed.schema.declaration(record).shape {
                    Shape::Enum { .. } => return Err(error(variant.line, variant.name, Wrong::NotRecord)),
                    Shape::Record { .. } => {}
                }
                parsed.schema.variants.get_mut(at).expect("variant belongs to this table").record = Some(record);
                let least = *parsed.schema.minimum.get(record.index()).expect("earlier record has its minimum");
                (u64::from(least), u64::from(parsed.schema.most_bytes(record)))
            }
            None => (0, 0),
        };
        minimum = minimum.min(least);
        maximum = maximum.max(most);
    }
    Ok((sum(1, minimum, declaration.line, declaration.name)?, sum(1, maximum, declaration.line, declaration.name)?))
}

pub(super) fn check(parsed: &mut Parsed) -> Result<(), SchemaError> {
    for at in 0..parsed.schema.declarations() {
        let declaration = parsed.schema.declaration(DeclIx::new(at));
        let (minimum, maximum) = match declaration.shape {
            Shape::Record { versioned, first, count } => record_size(parsed, versioned, first, count)?,
            Shape::Enum { first, count } => enum_size(parsed, declaration, first, count)?,
        };
        let name = parsed.schema.spelled(declaration.name);
        if parsed.schema.names.contains_key(name) {
            return Err(error(declaration.line, declaration.name, Wrong::DuplicateType));
        }
        let Ok(maximum) = u32::try_from(maximum) else {
            return Err(error(declaration.line, declaration.name, Wrong::Large));
        };
        let minimum = u32::try_from(minimum).expect("minimum does not exceed the checked maximum");
        parsed.schema.names.insert(bytes::copy_of(name), DeclIx::new(at)).expect("at most one name per declaration");
        parsed.schema.minimum.push(minimum).expect("at most one minimum per declaration");
        parsed.schema.maximum.push(maximum).expect("at most one maximum per declaration");
    }
    Ok(())
}
