//! Changes and proposals in text order (codec.md, 10.1–10.2).

use skein_codec::schema::{DeclIx, Schema, Shape, Type};

use super::compare::{Comparison, Types, spell};
use super::matching::{self, Pairs, Part, Value, name, part, parts};
use super::{Change, Diff, Fill, Kind, Op, Rename};

impl Diff {
    fn push(&mut self, kind: Kind, op: Op) {
        self.changes.push(Change { kind, op });
    }
}

fn same_part(types: &Types<'_>, kept: &Part, current: &Part) -> bool {
    match (kept.value, current.value) {
        (Value::Field(k), Value::Field(c)) => types.compare(k, c) == Comparison::Equal,
        (Value::Variant(k), Value::Variant(c)) => types.payload(k, c),
        (Value::Field(_), Value::Variant(_)) | (Value::Variant(_), Value::Field(_)) => false,
    }
}

fn same_layout(types: &Types<'_>, kept: u32, current: u32) -> bool {
    let k = types.kept.declaration(DeclIx::new(kept)).shape;
    let c = types.current.declaration(DeclIx::new(current)).shape;
    let same = match (k, c) {
        (Shape::Record { versioned: kv, count: kn, .. }, Shape::Record { versioned: cv, count: cn, .. }) => {
            kv == cv && kn == cn
        }
        (Shape::Enum { count: kn, .. }, Shape::Enum { count: cn, .. }) => kn == cn,
        (Shape::Record { .. }, Shape::Enum { .. }) | (Shape::Enum { .. }, Shape::Record { .. }) => false,
    };
    same && parts(types.kept, k).iter().zip(parts(types.current, c)).all(|(k, c)| same_part(types, k, &c))
}

pub(super) fn diff(kept: &Schema, current: &Schema, renames: &[Rename], pairs: &Pairs) -> Diff {
    let types = Types { kept, current, pairs };
    let mut result = Diff {
        family: matching::word(current, current.family()),
        kept: kept.version(),
        current: current.version(),
        changes: Vec::new(),
        proposed: Vec::new(),
    };
    let unmatched_kept: Vec<_> = (0..kept.declarations()).filter(|at| !pairs.values().any(|old| old == at)).collect();
    let unmatched_current = (0..current.declarations()).filter(|at| !pairs.contains_key(at));
    for (k, c) in unmatched_kept.iter().copied().zip(unmatched_current) {
        if same_layout(&types, k, c) {
            result.proposed.push(Rename::Declaration { old: name(kept, k), new: name(current, c) });
        }
    }
    for c in 0..current.declarations() {
        let decl = name(current, c);
        let Some(k) = pairs.get(&c).copied() else {
            result.push(Kind::NoneNeeded, Op::AddDeclaration { decl });
            continue;
        };
        let old = name(kept, k);
        if old != decl {
            result.push(Kind::NoneNeeded, Op::RenameDeclaration { old, new: decl.clone() });
        }
        let ks = kept.declaration(DeclIx::new(k)).shape;
        let cs = current.declaration(DeclIx::new(c)).shape;
        if !matching::same_shape(ks, cs) {
            result.push(Kind::Written, Op::Reshape { decl });
            continue;
        }
        if let (Shape::Record { versioned: kv, .. }, Shape::Record { versioned: cv, .. }) = (ks, cs)
            && kv != cv
        {
            result.push(Kind::Derived, Op::Versioned { decl: decl.clone(), versioned: cv });
        }
        walk_parts(&mut result, &types, &decl, &parts(kept, ks), &parts(current, cs), renames);
    }
    for k in unmatched_kept {
        result.push(Kind::NoneNeeded, Op::DropDeclaration { decl: name(kept, k) });
    }
    result
}

fn walk_parts(result: &mut Diff, types: &Types<'_>, decl: &str, kept: &[Part], current: &[Part], renames: &[Rename]) {
    let pairs = matching::pair_parts(kept, current, decl, renames);
    let unmatched_kept: Vec<_> = kept
        .iter()
        .enumerate()
        .filter(|(at, _)| !pairs.values().any(|old| *old == u32::try_from(*at).expect("bounded part index")))
        .map(|(_, part)| part)
        .collect();
    let unmatched_current = current
        .iter()
        .enumerate()
        .filter(|(at, _)| !pairs.contains_key(&u32::try_from(*at).expect("bounded part index")));
    for (k, (_, c)) in unmatched_kept.iter().zip(unmatched_current) {
        if same_part(types, k, c) {
            result.proposed.push(Rename::Part { decl: decl.into(), old: k.name.clone(), new: c.name.clone() });
        }
    }
    for (c, current_part) in current.iter().enumerate() {
        let c = u32::try_from(c).expect("bounded part index");
        if let Some(k) = pairs.get(&c) {
            let kept_part = part(kept, *k);
            if kept_part.name != current_part.name {
                result.push(
                    Kind::NoneNeeded,
                    Op::RenamePart { decl: decl.into(), old: kept_part.name.clone(), new: current_part.name.clone() },
                );
            }
            changed_part(result, types, decl, kept_part, current_part);
        } else {
            match current_part.value {
                Value::Field(ty) => {
                    let value =
                        if matches!(types.current.ty(ty), Type::Option(_)) { Fill::Absent } else { Fill::Asked };
                    result.push(
                        Kind::Derived,
                        Op::AddField {
                            decl: decl.into(),
                            field: current_part.name.clone(),
                            ty: spell(types.current, ty),
                            value,
                        },
                    );
                }
                Value::Variant(_) => {
                    let kind = if pairs.range(c..).next().is_some() { Kind::Derived } else { Kind::NoneNeeded };
                    result.push(kind, Op::AddVariant { decl: decl.into(), variant: current_part.name.clone() });
                }
            }
        }
    }
    for part in unmatched_kept {
        let (kind, op) = match part.value {
            Value::Field(_) => (Kind::Derived, Op::DropField { decl: decl.into(), field: part.name.clone() }),
            Value::Variant(_) => (Kind::Written, Op::DropVariant { decl: decl.into(), variant: part.name.clone() }),
        };
        result.push(kind, op);
    }
    if pairs.values().zip(pairs.values().skip(1)).any(|(a, b)| a > b) {
        let names = pairs.keys().map(|at| part(current, *at).name.clone()).collect();
        result.push(Kind::Derived, Op::Order { decl: decl.into(), parts: names });
    }
}

fn changed_part(result: &mut Diff, types: &Types<'_>, decl: &str, kept: &Part, current: &Part) {
    match (kept.value, current.value) {
        (Value::Field(k), Value::Field(c)) => {
            let comparison = types.compare(k, c);
            let field = current.name.clone();
            let kept = spell(types.kept, k);
            let current = spell(types.current, c);
            let decl = decl.into();
            let (kind, op) = match comparison {
                Comparison::Equal => return,
                Comparison::Widen(kind) => (kind, Op::Widen { decl, field, kept, current }),
                Comparison::Wrap => (Kind::Derived, Op::Wrap { decl, field, kept, current }),
                Comparison::Retype => (Kind::Written, Op::Retype { decl, field, kept, current }),
            };
            result.push(kind, op);
        }
        (Value::Variant(k), Value::Variant(c)) => {
            if !types.payload(k, c) {
                result.push(
                    Kind::Written,
                    Op::Payload {
                        decl: decl.into(),
                        variant: current.name.clone(),
                        kept: k.map(|at| name(types.kept, at.index())),
                        current: c.map(|at| name(types.current, at.index())),
                    },
                );
            }
        }
        (Value::Field(_), Value::Variant(_)) | (Value::Variant(_), Value::Field(_)) => {
            unreachable!("matched declarations have the same shape")
        }
    }
}
