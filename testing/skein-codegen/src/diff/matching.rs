//! Name pairing and rename validation (codec.md, 10.1).

use alloc::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use skein_codec::schema::{DeclIx, FieldIx, Schema, Shape, Span, TypeIx, VariantIx};

use super::{DiffError, Rename};

pub(super) type Pairs = BTreeMap<u32, u32>; // current index -> kept index

pub(super) fn word(schema: &Schema, span: Span) -> String {
    String::from_utf8(schema.spelled(span).to_vec()).expect("schema names are ASCII")
}

pub(super) fn name(schema: &Schema, at: u32) -> String {
    word(schema, schema.declaration(DeclIx::new(at)).name)
}

#[derive(Clone, Debug)]
pub(super) struct Part {
    pub name: String,
    pub value: Value,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Value {
    Field(TypeIx),
    Variant(Option<DeclIx>),
}

pub(super) fn parts(schema: &Schema, shape: Shape) -> Vec<Part> {
    match shape {
        Shape::Record { first, count, .. } => (first.index()
            ..first.index().checked_add(count).expect("bounded field table"))
            .map(|at| {
                let field = schema.field(FieldIx::new(at));
                Part { name: word(schema, field.name), value: Value::Field(field.ty) }
            })
            .collect(),
        Shape::Enum { first, count } => (first.index()
            ..first.index().checked_add(count).expect("bounded variant table"))
            .map(|at| {
                let variant = schema.variant(VariantIx::new(at));
                Part { name: word(schema, variant.name), value: Value::Variant(variant.record) }
            })
            .collect(),
    }
}

pub(super) fn same_shape(kept: Shape, current: Shape) -> bool {
    match (kept, current) {
        (Shape::Record { .. }, Shape::Record { .. }) | (Shape::Enum { .. }, Shape::Enum { .. }) => true,
        (Shape::Record { .. }, Shape::Enum { .. }) | (Shape::Enum { .. }, Shape::Record { .. }) => false,
    }
}

fn position(parts: &[Part], name: &str) -> Option<u32> {
    parts.iter().position(|part| part.name == name).map(|at| u32::try_from(at).expect("bounded parts table"))
}

fn by_name(kept: &[String], current: &[String], pairs: &mut Pairs) {
    let used: BTreeSet<_> = pairs.values().copied().collect();
    let remaining: BTreeMap<_, _> = kept
        .iter()
        .enumerate()
        .filter(|(at, _)| !used.contains(&u32::try_from(*at).expect("bounded table")))
        .map(|(at, name)| (name, u32::try_from(at).expect("bounded table")))
        .collect();
    for (at, name) in current.iter().enumerate() {
        let at = u32::try_from(at).expect("bounded table");
        if let Entry::Vacant(entry) = pairs.entry(at)
            && let Some(old) = remaining.get(name)
        {
            entry.insert(*old);
        }
    }
}

pub(super) fn declarations(kept: &Schema, current: &Schema, renames: &[Rename]) -> Result<Pairs, DiffError> {
    let mut pairs = Pairs::new();
    let mut old_names = BTreeSet::new();
    let mut new_names = BTreeSet::new();
    let mut errors = Vec::new();
    for (i, rename) in renames.iter().enumerate() {
        match rename {
            Rename::Declaration { old, new } => {
                let old_at = kept.find(old.as_bytes());
                let new_at = current.find(new.as_bytes());
                if let (Some(old_at), Some(new_at)) = (old_at, new_at) {
                    let fresh_old = old_names.insert(old);
                    let fresh_new = new_names.insert(new);
                    if fresh_old && fresh_new {
                        pairs.insert(new_at.index(), old_at.index());
                    } else {
                        errors.push(i);
                    }
                } else {
                    errors.push(i);
                }
            }
            Rename::Part { .. } => {}
        }
    }
    let kept_names: Vec<_> = (0..kept.declarations()).map(|at| name(kept, at)).collect();
    let current_names: Vec<_> = (0..current.declarations()).map(|at| name(current, at)).collect();
    by_name(&kept_names, &current_names, &mut pairs);
    let mut old_parts = BTreeSet::new();
    let mut new_parts = BTreeSet::new();
    for (i, rename) in renames.iter().enumerate() {
        match rename {
            Rename::Declaration { .. } => {}
            Rename::Part { decl, old, new } => {
                let valid = current
                    .find(decl.as_bytes())
                    .and_then(|at| {
                        let kept_at = pairs.get(&at.index())?;
                        let kept_shape = kept.declaration(DeclIx::new(*kept_at)).shape;
                        let current_shape = current.declaration(at).shape;
                        if !same_shape(kept_shape, current_shape) {
                            return None;
                        }
                        position(&parts(kept, kept_shape), old)?;
                        position(&parts(current, current_shape), new)?;
                        Some(())
                    })
                    .is_some();
                let fresh_old = old_parts.insert((decl, old));
                let fresh_new = new_parts.insert((decl, new));
                if !valid || !fresh_old || !fresh_new {
                    errors.push(i);
                }
            }
        }
    }
    if let Some(i) = errors.into_iter().min() { Err(DiffError::Rename(i)) } else { Ok(pairs) }
}

pub(super) fn pair_parts(kept: &[Part], current: &[Part], decl: &str, renames: &[Rename]) -> Pairs {
    let mut pairs = Pairs::new();
    for rename in renames {
        match rename {
            Rename::Declaration { .. } => {}
            Rename::Part { decl: scope, old, new } => {
                if scope == decl {
                    pairs.insert(
                        position(current, new).expect("validated current part"),
                        position(kept, old).expect("validated kept part"),
                    );
                }
            }
        }
    }
    let kept_names: Vec<_> = kept.iter().map(|part| part.name.clone()).collect();
    let current_names: Vec<_> = current.iter().map(|part| part.name.clone()).collect();
    by_name(&kept_names, &current_names, &mut pairs);
    pairs
}

pub(super) fn part(parts: &[Part], at: u32) -> &Part {
    parts.get(usize::try_from(at).expect("bounded part index")).expect("index belongs to parts")
}
