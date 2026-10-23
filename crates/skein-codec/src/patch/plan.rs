//! Matching, line checks and retained conversions (codec.md, 11.2–11.3).

use super::text::{self, Block, Kind, Line, LineKind, Parsed, Range, Token};
use super::{Missing, Op, PatchError, PatchWrong, Tables, Target, add, spelled};
use crate::schema::{DeclIx, FieldIx, Held, Schema, Shape, Span, Type, TypeIx, VariantIx};
use alloc::boxed::Box;
use skein_lib::{List, bytes};

#[cfg(test)]
mod tests;

/// Everything a run needs, with all indices owned by the indicated schema.
#[derive(Debug)]
pub(crate) struct Plan {
    /// Owner level; identity plans have no patch text and level zero.
    pub(crate) patch: u32,
    pub(crate) from: u16,
    pub(crate) to: u16,
    /// Indices in `Tables::schemas`.
    #[cfg_attr(not(test), expect(dead_code, reason = "the run reads its kept schema indices (codec.md, 11.4)"))]
    pub(crate) old: u32,
    #[cfg_attr(not(test), expect(dead_code, reason = "the run reads its kept schema indices (codec.md, 11.4)"))]
    pub(crate) new: u32,
    /// Refusal and mark spans refer to these bytes.
    pub(crate) text: Box<[u8]>,
    pub(crate) marks: List<Span>,
    /// One conversion per old declaration.
    pub(crate) declarations: List<Conversion>,
    /// One old declaration per new declaration.
    pub(crate) olds: List<Option<DeclIx>>,
    /// One source per new field, indexed by `FieldIx`.
    pub(crate) sources: List<Source>,
    /// One new variant per old variant.
    pub(crate) variants: List<Option<VariantIx>>,
    /// Whether carry may derive each old field and variant.
    pub(crate) fields: List<bool>,
    pub(crate) variant_derived: List<bool>,
    /// Added values, encoded once in canonical codec bytes.
    pub(crate) pool: List<u8>,
    pub(crate) sites: List<SitePlan>,
    /// Every refusal in the text, including other families' refusals.
    pub(crate) refusals: List<RefusalPlan>,
}

/// The whole-declaration conversion, computed in old declaration order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Conversion {
    pub(crate) new: Option<DeclIx>,
    /// Global site index, when the whole declaration is written.
    pub(crate) site: Option<u32>,
    /// The conversion can copy the encoded range at once.
    pub(crate) copies: bool,
    /// A site or held field is reachable by this conversion.
    pub(crate) stops: bool,
    /// Carry may derive this declaration without its own written function.
    pub(crate) derived: bool,
}

/// Where a new field obtains its value; site indices are text-global.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Old(FieldIx),
    Value {
        start: u32,
        end: u32,
    },
    Site(u32),
    /// An added `written` value supplied by the declaration's site.
    Written,
}

/// A site's target and half-open range in the text-global refusal table.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SitePlan {
    pub(crate) index: u32,
    pub(crate) target: Target,
    #[cfg_attr(not(test), expect(dead_code, reason = "the run checks a site's declared refusals (codec.md, 11.5)"))]
    pub(crate) refusals: (u32, u32),
}

/// A declared refusal's spans in `Plan::text` and its global site index.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RefusalPlan {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the run exposes declared refusal names and text (codec.md, 11.6)")
    )]
    pub(crate) name: Span,
    /// The content without its quotes.
    #[cfg_attr(not(test), expect(dead_code, reason = "the run exposes declared refusal text (codec.md, 11.6)"))]
    pub(crate) text: Span,
    pub(crate) site: u32,
}

/// Confirmed renames first, then unused names, all new indices to old.
#[derive(Debug)]
pub(crate) struct Matching {
    pub(crate) declarations: List<Option<DeclIx>>,
    pub(crate) fields: List<Option<FieldIx>>,
    pub(crate) variants: List<Option<VariantIx>>,
}

/// Level-by-level type comparison, reused by the run's carry operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Comparison {
    Equal,
    Widen { integer: bool },
    Wrap,
    Retype,
}

fn counts(schema: &Schema) -> (u32, u32) {
    let mut fields = 0;
    let mut variants = 0;
    for at in 0..schema.declarations() {
        match schema.declaration(DeclIx::new(at)).shape {
            Shape::Record { first, count, .. } => fields = add(first.index(), count),
            Shape::Enum { first, count } => variants = add(first.index(), count),
        }
    }
    (fields, variants)
}

fn empty_matching(new: &Schema) -> Matching {
    let (fields, variants) = counts(new);
    let mut matching = Matching {
        declarations: List::with_capacity(new.declarations()),
        fields: List::with_capacity(fields),
        variants: List::with_capacity(variants),
    };
    for _ in 0..new.declarations() {
        matching.declarations.push(None).expect("declaration room");
    }
    for _ in 0..fields {
        matching.fields.push(None).expect("field room");
    }
    for _ in 0..variants {
        matching.variants.push(None).expect("variant room");
    }
    matching
}

fn same_shape(old: Shape, new: Shape) -> bool {
    match old {
        Shape::Record { .. } => match new {
            Shape::Record { .. } => true,
            Shape::Enum { .. } => false,
        },
        Shape::Enum { .. } => match new {
            Shape::Enum { .. } => true,
            Shape::Record { .. } => false,
        },
    }
}
fn parts(shape: Shape) -> (u32, u32) {
    match shape {
        Shape::Record { first, count, .. } => (first.index(), count),
        Shape::Enum { first, count } => (first.index(), count),
    }
}
fn target(shape: Shape, at: u32) -> Target {
    match shape {
        Shape::Record { .. } => Target::Field(FieldIx::new(at)),
        Shape::Enum { .. } => Target::Variant(VariantIx::new(at)),
    }
}
fn part_name(schema: &Schema, part: Target) -> Span {
    match part {
        Target::Declaration(at) => schema.declaration(at).name,
        Target::Field(at) => schema.field(at).name,
        Target::Variant(at) => schema.variant(at).name,
    }
}
fn find_part(schema: &Schema, declaration: DeclIx, name: &[u8]) -> Option<Target> {
    let shape = schema.declaration(declaration).shape;
    let (first, count) = parts(shape);
    for at in first..add(first, count) {
        let part = target(shape, at);
        if schema.spelled(part_name(schema, part)) == name {
            return Some(part);
        }
    }
    None
}
impl Matching {
    pub(crate) fn old(&self, new: DeclIx) -> Option<DeclIx> {
        *self.declarations.get(new.index()).expect("new declaration index")
    }
    #[expect(clippy::manual_map, reason = "step code uses explicit matches without callbacks")]
    fn part(&self, new: Target) -> Option<Target> {
        match new {
            Target::Declaration(at) => match self.old(at) {
                Some(at) => Some(Target::Declaration(at)),
                None => None,
            },
            Target::Field(at) => match *self.fields.get(at.index()).expect("new field index") {
                Some(at) => Some(Target::Field(at)),
                None => None,
            },
            Target::Variant(at) => match *self.variants.get(at.index()).expect("new variant index") {
                Some(at) => Some(Target::Variant(at)),
                None => None,
            },
        }
    }
    fn used(&self, old: Target) -> bool {
        match old {
            Target::Declaration(at) => {
                for &value in &self.declarations {
                    if value == Some(at) {
                        return true;
                    }
                }
            }
            Target::Field(at) => {
                for &value in &self.fields {
                    if value == Some(at) {
                        return true;
                    }
                }
            }
            Target::Variant(at) => {
                for &value in &self.variants {
                    if value == Some(at) {
                        return true;
                    }
                }
            }
        }
        false
    }
    fn pair(&mut self, new: Target, old: Target) {
        match new {
            Target::Declaration(new) => match old {
                Target::Declaration(old) => {
                    *self.declarations.get_mut(new.index()).expect("new declaration index") = Some(old);
                }
                Target::Field(_) | Target::Variant(_) => unreachable!("same shaped pairing"),
            },
            Target::Field(new) => match old {
                Target::Field(old) => *self.fields.get_mut(new.index()).expect("new field index") = Some(old),
                Target::Declaration(_) | Target::Variant(_) => unreachable!("same shaped pairing"),
            },
            Target::Variant(new) => match old {
                Target::Variant(old) => *self.variants.get_mut(new.index()).expect("new variant index") = Some(old),
                Target::Declaration(_) | Target::Field(_) => unreachable!("same shaped pairing"),
            },
        }
    }
}

fn renamed_declaration(
    matching: &mut Matching,
    old: &Schema,
    new: &Schema,
    patch: &[u8],
    line: Line,
) -> Result<(), PatchError> {
    let old_at = old.find(spelled(patch, line.declaration)).ok_or(line.error(PatchWrong::Unknown))?;
    let new_at =
        new.find(spelled(patch, line.other)).ok_or(super::error(line.token.line, line.other, PatchWrong::Unknown))?;
    let old_target = Target::Declaration(old_at);
    let new_target = Target::Declaration(new_at);
    if matching.used(old_target) || matching.part(new_target).is_some() {
        return Err(line.error(PatchWrong::Twice));
    }
    if spelled(patch, line.declaration) == spelled(patch, line.other) {
        return Err(line.error(PatchWrong::Mismatch));
    }
    matching.pair(new_target, old_target);
    Ok(())
}
fn renamed_part(
    matching: &mut Matching,
    old: &Schema,
    new: &Schema,
    patch: &[u8],
    line: Line,
) -> Result<(), PatchError> {
    let new_decl = new.find(spelled(patch, line.declaration)).ok_or(line.error(PatchWrong::Unknown))?;
    let old_decl = matching.old(new_decl).ok_or(line.error(PatchWrong::Mismatch))?;
    if !same_shape(old.declaration(old_decl).shape, new.declaration(new_decl).shape) {
        return Err(line.error(PatchWrong::Mismatch));
    }
    let part = line.part.expect("part rename");
    let old_at = find_part(old, old_decl, spelled(patch, part)).ok_or(super::error(
        line.token.line,
        part,
        PatchWrong::Unknown,
    ))?;
    let new_at = find_part(new, new_decl, spelled(patch, line.other)).ok_or(super::error(
        line.token.line,
        line.other,
        PatchWrong::Unknown,
    ))?;
    if matching.used(old_at) || matching.part(new_at).is_some() {
        return Err(line.error(PatchWrong::Twice));
    }
    if spelled(patch, part) == spelled(patch, line.other) {
        return Err(line.error(PatchWrong::Mismatch));
    }
    matching.pair(new_at, old_at);
    Ok(())
}
fn matching(
    old: &Schema,
    new: &Schema,
    patch: &[u8],
    parsed: &Parsed,
    block: Block,
    errors: &mut List<Option<PatchError>>,
) -> Matching {
    let mut matching = empty_matching(new);
    for at in block.lines.start..block.lines.end {
        let line = *parsed.lines.get(at).expect("block line");
        if line.kind == LineKind::Rename
            && line.part.is_none()
            && let Err(error) = renamed_declaration(&mut matching, old, new, patch, line)
        {
            *errors.get_mut(at).expect("line error slot") = Some(error);
        }
    }
    for at in 0..new.declarations() {
        let new_at = DeclIx::new(at);
        if matching.old(new_at).is_none()
            && let Some(old_at) = old.find(new.spelled(new.declaration(new_at).name))
            && !matching.used(Target::Declaration(old_at))
        {
            matching.pair(Target::Declaration(new_at), Target::Declaration(old_at));
        }
    }
    for at in block.lines.start..block.lines.end {
        let line = *parsed.lines.get(at).expect("block line");
        if line.kind == LineKind::Rename
            && line.part.is_some()
            && let Err(error) = renamed_part(&mut matching, old, new, patch, line)
        {
            *errors.get_mut(at).expect("line error slot") = Some(error);
        }
    }
    for at in 0..new.declarations() {
        let new_at = DeclIx::new(at);
        let Some(old_at) = matching.old(new_at) else {
            continue;
        };
        let shape = new.declaration(new_at).shape;
        if !same_shape(old.declaration(old_at).shape, shape) {
            continue;
        }
        let (first, count) = parts(shape);
        for at in first..add(first, count) {
            let new_part = target(shape, at);
            if matching.part(new_part).is_none()
                && let Some(old_part) = find_part(old, old_at, new.spelled(part_name(new, new_part)))
                && !matching.used(old_part)
            {
                matching.pair(new_part, old_part);
            }
        }
    }
    matching
}

pub(crate) fn payload(matching: &List<Option<DeclIx>>, old: Option<DeclIx>, new: Option<DeclIx>) -> bool {
    match old {
        Some(old) => match new {
            Some(new) => *matching.get(new.index()).expect("new declaration index") == Some(old),
            None => false,
        },
        None => new.is_none(),
    }
}
fn held(matching: &List<Option<DeclIx>>, old: &Schema, new: &Schema, a: Held, b: Held) -> Comparison {
    match a {
        Held::Opaque => match b {
            Held::Opaque => Comparison::Equal,
            Held::Record { .. } | Held::Pieces(_) => Comparison::Widen { integer: false },
        },
        Held::Record { family: af, name: an } => match b {
            Held::Opaque => Comparison::Widen { integer: false },
            Held::Pieces(_) => Comparison::Retype,
            Held::Record { family: bf, name: bn } => {
                if old.spelled(af) == new.spelled(bf) && old.spelled(an) == new.spelled(bn) {
                    Comparison::Equal
                } else {
                    Comparison::Retype
                }
            }
        },
        Held::Pieces(a) => match b {
            Held::Opaque => Comparison::Widen { integer: false },
            Held::Record { .. } => Comparison::Retype,
            Held::Pieces(b) => {
                if payload(matching, Some(a), Some(b)) {
                    Comparison::Equal
                } else {
                    Comparison::Retype
                }
            }
        },
    }
}
fn width(ty: Type) -> Option<u32> {
    match ty {
        Type::U8 => Some(1),
        Type::U16 => Some(2),
        Type::U32 => Some(4),
        Type::U64 => Some(8),
        Type::Bool
        | Type::Duration
        | Type::Fixed(_)
        | Type::Secret(_)
        | Type::Text(_)
        | Type::Bytes { .. }
        | Type::List { .. }
        | Type::Option(_)
        | Type::Named(_) => None,
    }
}
#[expect(clippy::too_many_lines, reason = "the leaf comparison exhaustively lists schema type pairs (codec.md, 10.3)")]
fn leaf(matching: &List<Option<DeclIx>>, old: &Schema, new: &Schema, a: Type, b: Type) -> Comparison {
    let equal = match a {
        Type::Named(_) | Type::Bytes { .. } | Type::List { .. } | Type::Option(_) => false,
        Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Bool
        | Type::Duration
        | Type::Fixed(_)
        | Type::Secret(_)
        | Type::Text(_) => a == b,
    };
    if equal {
        return Comparison::Equal;
    }
    if let Some(a) = width(a)
        && let Some(b) = width(b)
    {
        return if b > a { Comparison::Widen { integer: true } } else { Comparison::Retype };
    }
    match a {
        Type::Named(a) => match b {
            Type::Named(b) => {
                if payload(matching, Some(a), Some(b)) {
                    Comparison::Equal
                } else {
                    Comparison::Retype
                }
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
            | Type::Bytes { .. }
            | Type::List { .. }
            | Type::Option(_) => Comparison::Retype,
        },
        Type::Text(a) => match b {
            Type::Text(b) | Type::Secret(b) | Type::Bytes { bound: b, .. } => {
                if b >= a {
                    Comparison::Widen { integer: false }
                } else {
                    Comparison::Retype
                }
            }
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::List { .. }
            | Type::Option(_)
            | Type::Named(_) => Comparison::Retype,
        },
        Type::Secret(a) => match b {
            Type::Secret(b) | Type::Bytes { bound: b, .. } => {
                if b >= a {
                    Comparison::Widen { integer: false }
                } else {
                    Comparison::Retype
                }
            }
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Text(_)
            | Type::List { .. }
            | Type::Option(_)
            | Type::Named(_) => Comparison::Retype,
        },
        Type::Bytes { bound: a, held: ah } => match b {
            Type::Secret(b) => {
                if b >= a {
                    Comparison::Widen { integer: false }
                } else {
                    Comparison::Retype
                }
            }
            Type::Bytes { bound: b, held: bh } => {
                if b < a {
                    return Comparison::Retype;
                }
                match held(matching, old, new, ah, bh) {
                    Comparison::Equal => {
                        if b > a {
                            Comparison::Widen { integer: false }
                        } else {
                            Comparison::Equal
                        }
                    }
                    other @ (Comparison::Widen { .. } | Comparison::Wrap | Comparison::Retype) => other,
                }
            }
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Text(_)
            | Type::List { .. }
            | Type::Option(_)
            | Type::Named(_) => Comparison::Retype,
        },
        Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Bool
        | Type::Duration
        | Type::Fixed(_)
        | Type::List { .. }
        | Type::Option(_) => Comparison::Retype,
    }
}

/// The diff's iterative comparison using new-to-old declaration matches.
/// A run passes `Plan::olds`, so comparison needs no planning scratch.
pub(crate) fn compare(
    matching: &List<Option<DeclIx>>,
    old: &Schema,
    new: &Schema,
    mut a: TypeIx,
    mut b: TypeIx,
) -> Comparison {
    let mut widening = false;
    let mut integer = false;
    let mut wrapped = false;
    let turns = add(
        u32::try_from(old.text().len()).expect("schema ceiling"),
        u32::try_from(new.text().len()).expect("schema ceiling"),
    );
    for _ in 0..turns {
        let av = old.ty(a);
        let bv = new.ty(b);
        match bv {
            Type::Option(inner) => {
                b = inner;
                match av {
                    Type::Option(inner) => a = inner,
                    Type::U8
                    | Type::U16
                    | Type::U32
                    | Type::U64
                    | Type::Bool
                    | Type::Duration
                    | Type::Fixed(_)
                    | Type::Secret(_)
                    | Type::Text(_)
                    | Type::Bytes { .. }
                    | Type::List { .. }
                    | Type::Named(_) => wrapped = true,
                }
                continue;
            }
            Type::List { bound: bb, item: bi } => match av {
                Type::List { bound: ab, item: ai } => {
                    if bb < ab {
                        return Comparison::Retype;
                    }
                    widening |= bb > ab;
                    a = ai;
                    b = bi;
                    continue;
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
                | Type::Bytes { .. }
                | Type::Option(_)
                | Type::Named(_) => {}
            },
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Secret(_)
            | Type::Text(_)
            | Type::Bytes { .. }
            | Type::Named(_) => {}
        }
        match leaf(matching, old, new, av, bv) {
            Comparison::Retype => return Comparison::Retype,
            Comparison::Equal => {}
            Comparison::Widen { integer: wider } => {
                widening = true;
                integer |= wider;
            }
            Comparison::Wrap => unreachable!("a leaf does not wrap"),
        }
        return if wrapped {
            Comparison::Wrap
        } else if widening {
            Comparison::Widen { integer }
        } else {
            Comparison::Equal
        };
    }
    unreachable!("each wrapper advances within its schema's bounded types")
}

#[derive(Clone, Copy, Debug)]
struct Change {
    op: Op,
    target: Target,
    implicit: bool,
    written: Option<Target>,
    seen: bool,
}
fn change(changes: &mut List<Change>, op: Op, target: Target, implicit: bool, written: Option<Target>) {
    changes.push(Change { op, target, implicit, written, seen: false }).expect("changes bounded by both schema texts");
}
fn dropped_parts(changes: &mut List<Change>, matching: &Matching, shape: Shape, declaration: Target) {
    let (first, count) = parts(shape);
    for at in first..add(first, count) {
        let old_part = target(shape, at);
        if !matching.used(old_part) {
            let written = match old_part {
                Target::Variant(_) => Some(declaration),
                Target::Field(_) => None,
                Target::Declaration(_) => unreachable!("part target"),
            };
            change(changes, Op::Drop, old_part, false, written);
        }
    }
}

fn changes(old: &Schema, new: &Schema, matching: &Matching) -> List<Change> {
    let room = add(
        u32::try_from(old.text().len()).expect("schema ceiling"),
        u32::try_from(new.text().len()).expect("schema ceiling"),
    );
    let mut changes = List::with_capacity(room);
    for at in 0..new.declarations() {
        let new_at = DeclIx::new(at);
        let decl = Target::Declaration(new_at);
        let Some(old_at) = matching.old(new_at) else {
            change(&mut changes, Op::Add, decl, false, None);
            continue;
        };
        let a = old.declaration(old_at).shape;
        let b = new.declaration(new_at).shape;
        if !same_shape(a, b) {
            change(&mut changes, Op::Reshape, decl, false, Some(decl));
            continue;
        }
        match a {
            Shape::Record { versioned: av, .. } => match b {
                Shape::Record { versioned: bv, .. } => {
                    if av != bv {
                        change(&mut changes, if bv { Op::Versioned } else { Op::Unversioned }, decl, false, None);
                    }
                }
                Shape::Enum { .. } => unreachable!("same shape"),
            },
            Shape::Enum { .. } => {}
        }
        let (first, count) = parts(b);
        let mut previous = None;
        let mut reordered = false;
        for at in first..add(first, count) {
            let new_part = target(b, at);
            let Some(old_part) = matching.part(new_part) else {
                let mut implicit = false;
                match new_part {
                    Target::Variant(_) => {
                        implicit = true;
                        for later in add(at, 1)..add(first, count) {
                            if matching.part(target(b, later)).is_some() {
                                implicit = false;
                                break;
                            }
                        }
                    }
                    Target::Field(_) => {}
                    Target::Declaration(_) => unreachable!("part target"),
                }
                change(&mut changes, Op::Add, new_part, implicit, None);
                continue;
            };
            let index = match old_part {
                Target::Field(at) => at.index(),
                Target::Variant(at) => at.index(),
                Target::Declaration(_) => unreachable!("part target"),
            };
            if let Some(prev) = previous {
                reordered |= prev > index;
            }
            previous = Some(index);
            match new_part {
                Target::Field(new_at) => match old_part {
                    Target::Field(old_at) => {
                        match compare(&matching.declarations, old, new, old.field(old_at).ty, new.field(new_at).ty) {
                            Comparison::Equal => {}
                            Comparison::Widen { integer } => change(&mut changes, Op::Widen, new_part, !integer, None),
                            Comparison::Wrap => change(&mut changes, Op::Wrap, new_part, false, None),
                            Comparison::Retype => change(&mut changes, Op::Retype, new_part, false, Some(new_part)),
                        }
                    }
                    Target::Declaration(_) | Target::Variant(_) => unreachable!("same shaped pair"),
                },
                Target::Variant(new_at) => match old_part {
                    Target::Variant(old_at) => {
                        if !payload(&matching.declarations, old.variant(old_at).record, new.variant(new_at).record) {
                            change(&mut changes, Op::Payload, new_part, false, Some(decl));
                        }
                    }
                    Target::Declaration(_) | Target::Field(_) => unreachable!("same shaped pair"),
                },
                Target::Declaration(_) => unreachable!("part target"),
            }
        }
        dropped_parts(&mut changes, matching, a, decl);
        if reordered {
            change(&mut changes, Op::Order, decl, false, None);
        }
    }
    for at in 0..old.declarations() {
        let target = Target::Declaration(DeclIx::new(at));
        if !matching.used(target) {
            change(&mut changes, Op::Drop, target, false, None);
        }
    }
    changes
}

fn token(parsed: &Parsed, at: u32) -> Token {
    *parsed.tokens.get(at).expect("token range belongs to parsed text")
}
fn word(parsed: &Parsed, at: u32) -> Span {
    token(parsed, at).at
}

// A spelled type is compared against a schema chain; no schema internals are needed.
fn spells(patch: &[u8], parsed: &Parsed, range: Range, schema: &Schema, mut ty: TypeIx) -> bool {
    let mut at = range.start;
    for _ in range.start..range.end {
        if at >= range.end {
            return false;
        }
        let name = spelled(patch, word(parsed, at));
        at = add(at, 1);
        let mut bound = None;
        let mut next = None;
        let right = match schema.ty(ty) {
            Type::U8 => name == b"u8",
            Type::U16 => name == b"u16",
            Type::U32 => name == b"u32",
            Type::U64 => name == b"u64",
            Type::Bool => name == b"bool",
            Type::Duration => name == b"duration",
            Type::Fixed(n) => {
                bound = Some(n);
                name == b"fixed"
            }
            Type::Secret(n) => {
                bound = Some(n);
                name == b"secret"
            }
            Type::Text(n) => {
                bound = Some(n);
                name == b"text"
            }
            Type::List { bound: n, item } => {
                bound = Some(n);
                next = Some(item);
                name == b"list"
            }
            Type::Option(item) => {
                next = Some(item);
                name == b"option"
            }
            Type::Named(decl) => name == schema.spelled(schema.declaration(decl).name),
            Type::Bytes { bound: n, .. } => {
                bound = Some(n);
                name == b"bytes"
            }
        };
        if !right {
            return false;
        }
        if let Some(bound) = bound {
            if at >= range.end || text::decimal(spelled(patch, word(parsed, at))) != Some(u64::from(bound)) {
                return false;
            }
            at = add(at, 1);
        }
        match schema.ty(ty) {
            Type::Bytes { held, .. } => match held {
                Held::Opaque => {}
                Held::Record { family, name } => {
                    if add(at, 3) > range.end
                        || spelled(patch, word(parsed, at)) != b"holds"
                        || spelled(patch, word(parsed, add(at, 1))) != schema.spelled(family)
                        || spelled(patch, word(parsed, add(at, 2))) != schema.spelled(name)
                    {
                        return false;
                    }
                    at = add(at, 3);
                }
                Held::Pieces(decl) => {
                    if add(at, 2) > range.end
                        || spelled(patch, word(parsed, at)) != b"pieces"
                        || spelled(patch, word(parsed, add(at, 1))) != schema.spelled(schema.declaration(decl).name)
                    {
                        return false;
                    }
                    at = add(at, 2);
                }
            },
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Secret(_)
            | Type::Text(_)
            | Type::List { .. }
            | Type::Option(_)
            | Type::Named(_) => {}
        }
        match next {
            Some(item) => ty = item,
            None => return at == range.end,
        }
    }
    false
}

fn resolve(schema: &Schema, patch: &[u8], line: Line) -> Result<(DeclIx, Target), PatchError> {
    let decl = schema.find(spelled(patch, line.declaration)).ok_or(line.error(PatchWrong::Unknown))?;
    match line.part {
        Some(part) => Ok((
            decl,
            find_part(schema, decl, spelled(patch, part)).ok_or(super::error(
                line.token.line,
                part,
                PatchWrong::Unknown,
            ))?,
        )),
        None => Ok((decl, Target::Declaration(decl))),
    }
}
fn site_for(plan: &Plan, target: Target) -> Option<u32> {
    for site in &plan.sites {
        if site.target == target {
            return Some(site.index);
        }
    }
    None
}

// Dependencies may follow their use. Resolve only their targets here, leaving
// every line's own refusal to the text-order validation pass.
fn has_site(new: &Schema, patch: &[u8], parsed: &Parsed, block: Block, target: Target) -> bool {
    for at in block.lines.start..block.lines.end {
        let line = *parsed.lines.get(at).expect("block line");
        if line.kind == LineKind::Site
            && let Ok((_, found)) = resolve(new, patch, line)
            && found == target
        {
            return true;
        }
    }
    false
}

fn written_add(new: &Schema, patch: &[u8], parsed: &Parsed, block: Block, target: Target) -> bool {
    for at in block.lines.start..block.lines.end {
        let line = *parsed.lines.get(at).expect("block line");
        if line.kind == LineKind::Operation(Op::Add)
            && line.part.is_some()
            && let Ok((_, found)) = resolve(new, patch, line)
            && found == target
            && line.value.end == add(line.value.start, 1)
            && spelled(patch, word(parsed, line.value.start)) == b"written"
        {
            return true;
        }
    }
    false
}
fn check_site(
    plan: &Plan,
    matching: &Matching,
    new: &Schema,
    decl: DeclIx,
    target: Target,
    line: Line,
) -> Result<(), PatchError> {
    if matching.old(decl).is_none() {
        return Err(line.error(PatchWrong::Site));
    }
    if site_for(plan, target).is_some() {
        return Err(line.error(PatchWrong::Twice));
    }
    match target {
        Target::Declaration(_) => {
            let shape = new.declaration(decl).shape;
            let (first, count) = parts(shape);
            for at in first..add(first, count) {
                if site_for(plan, self::target(shape, at)).is_some() {
                    return Err(line.error(PatchWrong::Site));
                }
            }
        }
        Target::Field(_) => {
            if site_for(plan, Target::Declaration(decl)).is_some() {
                return Err(line.error(PatchWrong::Site));
            }
        }
        Target::Variant(_) => return Err(line.error(PatchWrong::Site)),
    }
    Ok(())
}
fn record_spells(patch: &[u8], parsed: &Parsed, range: Range, schema: &Schema, record: Option<DeclIx>) -> bool {
    let value = spelled(patch, word(parsed, range.start));
    match record {
        Some(at) => value == schema.spelled(schema.declaration(at).name),
        None => value == b"-",
    }
}
fn check_operation(
    old: &Schema,
    new: &Schema,
    matching: &Matching,
    patch: &[u8],
    parsed: &Parsed,
    line: Line,
    target: Target,
) -> Result<(), PatchError> {
    let op = match line.kind {
        LineKind::Operation(op) => op,
        LineKind::Rename | LineKind::Site | LineKind::Refusal => unreachable!("operation line"),
    };
    let mismatch = line.error(PatchWrong::Mismatch);
    match op {
        Op::Add => match target {
            Target::Field(at) => {
                if !spells(patch, parsed, line.new_type, new, new.field(at).ty) {
                    return Err(mismatch);
                }
            }
            Target::Variant(_) | Target::Declaration(_) => {
                if line.new_type.start != line.new_type.end {
                    return Err(mismatch);
                }
            }
        },
        Op::Widen | Op::Wrap | Op::Retype => match target {
            Target::Field(at) => {
                let Some(old_at) = *matching.fields.get(at.index()).expect("new field") else {
                    return Err(mismatch);
                };
                if !spells(patch, parsed, line.old_type, old, old.field(old_at).ty)
                    || !spells(patch, parsed, line.new_type, new, new.field(at).ty)
                {
                    return Err(mismatch);
                }
            }
            Target::Declaration(_) | Target::Variant(_) => return Err(mismatch),
        },
        Op::Payload => match target {
            Target::Variant(at) => {
                let Some(old_at) = *matching.variants.get(at.index()).expect("new variant") else {
                    return Err(mismatch);
                };
                if !record_spells(patch, parsed, line.old_type, old, old.variant(old_at).record)
                    || !record_spells(patch, parsed, line.new_type, new, new.variant(at).record)
                {
                    return Err(mismatch);
                }
            }
            Target::Declaration(_) | Target::Field(_) => return Err(mismatch),
        },
        Op::Order => {
            let decl = match target {
                Target::Declaration(decl) => decl,
                Target::Field(_) | Target::Variant(_) => return Err(mismatch),
            };
            let shape = new.declaration(decl).shape;
            let (first, count) = parts(shape);
            let mut at = line.value.start;
            for index in first..add(first, count) {
                let part = self::target(shape, index);
                if matching.part(part).is_some() {
                    if at >= line.value.end || spelled(patch, word(parsed, at)) != new.spelled(part_name(new, part)) {
                        return Err(mismatch);
                    }
                    at = add(at, 1);
                }
            }
            if at != line.value.end {
                return Err(mismatch);
            }
        }
        Op::Drop | Op::Reshape | Op::Versioned | Op::Unversioned => {}
        Op::Written => unreachable!("written is a site"),
    }
    Ok(())
}

fn emit(pool: &mut List<u8>, byte: u8) {
    pool.push(byte).expect("encoded values bounded before allocation");
}
fn uint(pool: &mut List<u8>, value: u64, width: u32) {
    let bytes = value.to_be_bytes();
    let start = 8_u32.checked_sub(width).expect("integer width at most eight");
    for at in start..8 {
        emit(pool, *bytes.get(usize::try_from(at).expect("u32 fits usize")).expect("integer byte index"));
    }
}
fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => byte.checked_sub(b'0'),
        b'a'..=b'f' => byte.checked_sub(b'a')?.checked_add(10),
        _ => None,
    }
}
fn encoded_hex(pool: &mut List<u8>, value: &[u8], bound: u32, fixed: bool, pieces: bool) -> Result<(), PatchWrong> {
    if !value.starts_with(b"0x") {
        return Err(PatchWrong::Value);
    }
    let digits = value.get(2..).expect("hex prefix checked");
    let length = u32::try_from(digits.len()).expect("patch ceiling");
    if length.checked_rem(2) != Some(0) {
        return Err(PatchWrong::Value);
    }
    let length = length.checked_div(2).expect("nonzero divisor");
    if length > bound || (fixed && length != bound) || (pieces && length != 0) {
        return Err(PatchWrong::Value);
    }
    if !fixed {
        uint(pool, u64::from(length), 4);
    }
    for pair in digits.chunks_exact(2) {
        let a = hex(*pair.first().expect("two hex digits")).ok_or(PatchWrong::Value)?;
        let b = hex(*pair.get(1).expect("two hex digits")).ok_or(PatchWrong::Value)?;
        emit(pool, a.checked_mul(16).expect("hex digit").checked_add(b).expect("hex byte"));
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "the value encoder exhaustively lists the accepted forms of every schema type (codec.md, 11.2)"
)]
fn value(
    pool: &mut List<u8>,
    schema: &Schema,
    mut ty: TypeIx,
    patch: &[u8],
    parsed: &Parsed,
    range: Range,
) -> Result<Source, PatchError> {
    let start = pool.len();
    for at in range.start..range.end {
        let token = token(parsed, at);
        let word = spelled(patch, token.at);
        let wrong = token.error(PatchWrong::Value);
        if word == b"?" {
            return Err(token.error(PatchWrong::Asked));
        }
        if word == b"written" {
            if at != range.start {
                return Err(wrong);
            }
            return Ok(Source::Written);
        }
        if word == b"some" {
            match schema.ty(ty) {
                Type::Option(inner) => {
                    emit(pool, 1);
                    ty = inner;
                    continue;
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
                | Type::Bytes { .. }
                | Type::List { .. }
                | Type::Named(_) => return Err(wrong),
            }
        }
        if word == b"none" {
            match schema.ty(ty) {
                Type::Option(_) => emit(pool, 0),
                Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::Bool
                | Type::Duration
                | Type::Fixed(_)
                | Type::Secret(_)
                | Type::Text(_)
                | Type::Bytes { .. }
                | Type::List { .. }
                | Type::Named(_) => return Err(wrong),
            }
        } else {
            match schema.ty(ty) {
                integer @ (Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Duration) => {
                    let width = match integer {
                        Type::Duration => 8,
                        Type::U8 | Type::U16 | Type::U32 | Type::U64 => width(integer).expect("integer type"),
                        Type::Bool
                        | Type::Fixed(_)
                        | Type::Secret(_)
                        | Type::Text(_)
                        | Type::Bytes { .. }
                        | Type::List { .. }
                        | Type::Option(_)
                        | Type::Named(_) => unreachable!("integer type"),
                    };
                    let value = text::decimal(word).ok_or(wrong)?;
                    let maximum = match width {
                        1 => u64::from(u8::MAX),
                        2 => u64::from(u16::MAX),
                        4 => u64::from(u32::MAX),
                        8 => u64::MAX,
                        _ => unreachable!("integer width"),
                    };
                    if value > maximum {
                        return Err(wrong);
                    }
                    uint(pool, value, width);
                }
                Type::Bool => {
                    if word == b"true" {
                        emit(pool, 1);
                    } else if word == b"false" {
                        emit(pool, 0);
                    } else {
                        return Err(wrong);
                    }
                }
                Type::Fixed(bound) => {
                    if let Err(wrong) = encoded_hex(pool, word, bound, true, false) {
                        return Err(token.error(wrong));
                    }
                }
                Type::Bytes { bound, held } => {
                    let pieces = match held {
                        Held::Opaque => false,
                        Held::Pieces(_) => true,
                        Held::Record { .. } => return Err(wrong),
                    };
                    if let Err(wrong) = encoded_hex(pool, word, bound, false, pieces) {
                        return Err(token.error(wrong));
                    }
                }
                Type::Text(bound) => {
                    if token.kind != Kind::Text {
                        return Err(wrong);
                    }
                    let content = spelled(
                        patch,
                        Span { start: add(token.at.start, 1), end: token.at.end.checked_sub(1).expect("quoted text") },
                    );
                    let length = u32::try_from(content.len()).expect("patch ceiling");
                    if length > bound || !crate::text_is_valid(content) {
                        return Err(wrong);
                    }
                    uint(pool, u64::from(length), 4);
                    for &byte in content {
                        emit(pool, byte);
                    }
                }
                Type::List { .. } => {
                    if word != b"[" {
                        return Err(wrong);
                    }
                    uint(pool, 0, 4);
                }
                Type::Named(decl) => match schema.declaration(decl).shape {
                    Shape::Record { .. } => return Err(wrong),
                    Shape::Enum { first, count } => {
                        let mut selected = None;
                        for index in 0..count {
                            let variant = schema.variant(VariantIx::new(add(first.index(), index)));
                            if word == schema.spelled(variant.name) && variant.record.is_none() {
                                selected = Some(index);
                                break;
                            }
                        }
                        emit(pool, u8::try_from(selected.ok_or(wrong)?).expect("enum tag at most 255"));
                    }
                },
                Type::Secret(_) | Type::Option(_) => return Err(wrong),
            }
        }
        return Ok(Source::Value { start, end: pool.len() });
    }
    Err(token(parsed, range.start).error(PatchWrong::Value))
}

fn empty_plan(old: &Schema, new: &Schema, old_index: u32, new_index: u32, room: u32, pool: u32) -> Plan {
    let (old_fields, old_variants) = counts(old);
    let (new_fields, _) = counts(new);
    let mut plan = Plan {
        patch: 0,
        from: old.version(),
        to: new.version(),
        old: old_index,
        new: new_index,
        text: Box::default(),
        marks: List::with_capacity(room),
        declarations: List::with_capacity(old.declarations()),
        olds: List::with_capacity(new.declarations()),
        sources: List::with_capacity(new_fields),
        variants: List::with_capacity(old_variants),
        fields: List::with_capacity(old_fields),
        variant_derived: List::with_capacity(old_variants),
        pool: List::with_capacity(pool),
        sites: List::with_capacity(room),
        refusals: List::with_capacity(room),
    };
    for _ in 0..old.declarations() {
        plan.declarations
            .push(Conversion { new: None, site: None, copies: false, stops: false, derived: false })
            .expect("old declarations room");
    }
    for _ in 0..new.declarations() {
        plan.olds.push(None).expect("new declarations room");
    }
    for _ in 0..new_fields {
        plan.sources.push(Source::Written).expect("new fields room");
    }
    for _ in 0..old_fields {
        plan.fields.push(false).expect("old fields room");
    }
    for _ in 0..old_variants {
        plan.variants.push(None).expect("old variants room");
        plan.variant_derived.push(false).expect("old variants room");
    }
    plan
}

fn check_missing(plan: &Plan, new: &Schema, changes: &List<Change>, block: Block) -> Result<(), PatchError> {
    for change in changes {
        if !change.seen && !change.implicit {
            return Err(block.error(PatchWrong::Missing(Missing { op: change.op, target: change.target })));
        }
        if let Some(target) = change.written {
            let mut present = site_for(plan, target).is_some();
            match target {
                Target::Field(_) => {
                    for at in 0..new.declarations() {
                        let decl = DeclIx::new(at);
                        let shape = new.declaration(decl).shape;
                        let (first, count) = parts(shape);
                        for at in first..add(first, count) {
                            if self::target(shape, at) == target {
                                present |= site_for(plan, Target::Declaration(decl)).is_some();
                            }
                        }
                    }
                }
                Target::Declaration(_) | Target::Variant(_) => {}
            }
            if !present {
                return Err(block.error(PatchWrong::Missing(Missing { op: Op::Written, target })));
            }
        }
    }
    Ok(())
}

// Marks and every family's refusal spans share the one owned patch text.
fn keep_text(plan: &mut Plan, patch: &[u8], parsed: &Parsed) {
    plan.patch = parsed.header.level;
    plan.text = bytes::copy_of(patch);
    for &mark in &parsed.marks {
        plan.marks.push(mark).expect("mark room");
    }
    for line in &parsed.lines {
        if line.kind == LineKind::Refusal {
            plan.refusals
                .push(RefusalPlan {
                    name: line.declaration,
                    text: Span {
                        start: add(line.other.start, 1),
                        end: line.other.end.checked_sub(1).expect("quoted refusal"),
                    },
                    site: line.site,
                })
                .expect("refusals bounded by tokens");
        }
    }
}

pub(super) fn build(tables: &Tables, patch: &[u8], parsed: &Parsed, block: Block) -> Result<Plan, PatchError> {
    let old_index = tables.schema_index(block.from).ok_or(block.error(PatchWrong::Unkept))?;
    let new_index = tables.schema_index(block.to).ok_or(block.error(PatchWrong::Unkept))?;
    if block.from.checked_add(1) != Some(block.to) {
        return Err(block.error(PatchWrong::Version));
    }
    let old = tables.schemas.get(old_index).expect("kept schema index");
    let new = tables.schemas.get(new_index).expect("kept schema index");
    let mut errors = List::with_capacity(parsed.lines.len());
    for _ in 0..parsed.lines.len() {
        errors.push(None).expect("one error slot per line");
    }
    let matching = matching(old, new, patch, parsed, block, &mut errors);
    let mut changes = changes(old, new, &matching);
    // Each numeric value encodes in at most eight bytes per value token;
    // text and hex contents encode in no more than their text's byte count.
    let pool_room = parsed
        .tokens
        .len()
        .checked_mul(8)
        .expect("tokens bounded by text ceiling")
        .checked_add(u32::try_from(patch.len()).expect("patch length checked"))
        .expect("encoded values bounded by nine times text");
    let mut plan = empty_plan(old, new, old_index, new_index, parsed.tokens.len(), pool_room);
    keep_text(&mut plan, patch, parsed);
    for at in block.lines.start..block.lines.end {
        let line = *parsed.lines.get(at).expect("block line");
        if let Some(error) = *errors.get(at).expect("line error slot") {
            return Err(error);
        }
        match line.kind {
            LineKind::Rename | LineKind::Refusal => {}
            LineKind::Site => {
                let (decl, target) = resolve(new, patch, line)?;
                check_site(&plan, &matching, new, decl, target, line)?;
                match target {
                    Target::Field(_) => {
                        if matching.part(target).is_none() && !written_add(new, patch, parsed, block, target) {
                            return Err(line.error(PatchWrong::Site));
                        }
                    }
                    Target::Declaration(_) | Target::Variant(_) => {}
                }
                let mut first = plan.refusals.len();
                let mut end = first;
                for (index, refusal) in plan.refusals.iter().enumerate() {
                    if refusal.site == line.index {
                        let index = u32::try_from(index).expect("bounded refusal index");
                        first = first.min(index);
                        end = add(index, 1);
                    }
                }
                plan.sites.push(SitePlan { index: line.index, target, refusals: (first, end) }).expect("site room");
            }
            LineKind::Operation(op) => {
                let schema = if op == Op::Drop { old } else { new };
                let (decl, target) = resolve(schema, patch, line)?;
                let mut found = None;
                for (index, change) in changes.iter().enumerate() {
                    if change.op == op && change.target == target {
                        found = Some(u32::try_from(index).expect("bounded changes"));
                        break;
                    }
                }
                let index = found.ok_or(line.error(PatchWrong::Mismatch))?;
                let change = changes.get_mut(index).expect("change index");
                if change.seen {
                    return Err(line.error(PatchWrong::Twice));
                }
                check_operation(old, new, &matching, patch, parsed, line, target)?;
                change.seen = true;
                if op == Op::Add {
                    match target {
                        Target::Field(field) => {
                            let source = value(&mut plan.pool, new, new.field(field).ty, patch, parsed, line.value)?;
                            if source == Source::Written
                                && !has_site(new, patch, parsed, block, target)
                                && !has_site(new, patch, parsed, block, Target::Declaration(decl))
                            {
                                return Err(line.error(PatchWrong::Site));
                            }
                            *plan.sources.get_mut(field.index()).expect("new field source") = source;
                        }
                        Target::Declaration(_) | Target::Variant(_) => {}
                    }
                }
            }
        }
    }
    check_missing(&plan, new, &changes, block)?;
    if let Err(wrong) = fill(&mut plan, old, new, &matching, false) {
        return Err(block.error(wrong));
    }
    Ok(plan)
}

fn conversion(plan: &Plan, at: DeclIx) -> Conversion {
    *plan.declarations.get(at.index()).expect("old declaration conversion")
}
#[expect(
    clippy::too_many_lines,
    reason = "the bounded wrapper walk exhaustively names the terminal schema types (codec.md, 11.3)"
)]
fn reference_flags(
    plan: &Plan,
    old: &Schema,
    new: &Schema,
    matching: &Matching,
    mut a: TypeIx,
    mut b: TypeIx,
) -> Result<(bool, bool), PatchWrong> {
    let copies = match compare(&matching.declarations, old, new, a, b) {
        Comparison::Equal | Comparison::Widen { integer: false } => true,
        Comparison::Widen { integer: true } | Comparison::Wrap | Comparison::Retype => false,
    };
    let bound = add(
        u32::try_from(old.text().len()).expect("schema ceiling"),
        u32::try_from(new.text().len()).expect("schema ceiling"),
    );
    for _ in 0..bound {
        let av = old.ty(a);
        let bv = new.ty(b);
        match bv {
            Type::Option(inner) => {
                b = inner;
                match av {
                    Type::Option(inner) => a = inner,
                    Type::U8
                    | Type::U16
                    | Type::U32
                    | Type::U64
                    | Type::Bool
                    | Type::Duration
                    | Type::Fixed(_)
                    | Type::Secret(_)
                    | Type::Text(_)
                    | Type::Bytes { .. }
                    | Type::List { .. }
                    | Type::Named(_) => {}
                }
                continue;
            }
            Type::List { item, .. } => match av {
                Type::List { item: old, .. } => {
                    a = old;
                    b = item;
                    continue;
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
                | Type::Bytes { .. }
                | Type::Option(_)
                | Type::Named(_) => {}
            },
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Secret(_)
            | Type::Text(_)
            | Type::Bytes { .. }
            | Type::Named(_) => {}
        }
        match av {
            Type::Named(at) => {
                let converted = conversion(plan, at);
                return Ok((copies && converted.copies, converted.stops));
            }
            Type::Bytes { held: Held::Pieces(at), .. } => {
                if !conversion(plan, at).copies {
                    return Err(PatchWrong::Pieces);
                }
                return Ok((copies, false));
            }
            Type::Bytes { held: Held::Record { family: af, name: an }, .. } => match bv {
                Type::Bytes { held: Held::Record { family: bf, name: bn }, .. } => {
                    let stops = old.spelled(af) != old.spelled(old.family())
                        && old.spelled(af) == new.spelled(bf)
                        && old.spelled(an) == new.spelled(bn);
                    return Ok((copies && !stops, stops));
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
                | Type::Bytes { held: Held::Opaque | Held::Pieces(_), .. }
                | Type::List { .. }
                | Type::Option(_)
                | Type::Named(_) => return Ok((copies, false)),
            },
            Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Duration
            | Type::Fixed(_)
            | Type::Secret(_)
            | Type::Text(_)
            | Type::Bytes { held: Held::Opaque, .. }
            | Type::List { .. }
            | Type::Option(_) => return Ok((copies, false)),
        }
    }
    unreachable!("wrapper chains bounded by schema text")
}

fn fill_matches(plan: &mut Plan, new: &Schema, matching: &Matching) {
    for at in 0..new.declarations() {
        let new_at = DeclIx::new(at);
        let old_at = matching.old(new_at);
        *plan.olds.get_mut(at).expect("new declaration slot") = old_at;
        if let Some(old_at) = old_at {
            let site = site_for(plan, Target::Declaration(new_at));
            let converted = plan.declarations.get_mut(old_at.index()).expect("old declaration slot");
            converted.new = Some(new_at);
            converted.site = site;
        }
    }
    for at in 0..matching.fields.len() {
        let new_at = FieldIx::new(at);
        if let Some(site) = site_for(plan, Target::Field(new_at)) {
            *plan.sources.get_mut(at).expect("new field source") = Source::Site(site);
        } else if let Some(old_at) = *matching.fields.get(at).expect("new field match") {
            *plan.sources.get_mut(at).expect("new field source") = Source::Old(old_at);
        }
    }
    for at in 0..matching.variants.len() {
        if let Some(old_at) = *matching.variants.get(at).expect("new variant match") {
            *plan.variants.get_mut(old_at.index()).expect("old variant slot") = Some(VariantIx::new(at));
        }
    }
}

fn fill(plan: &mut Plan, old: &Schema, new: &Schema, matching: &Matching, identity: bool) -> Result<(), PatchWrong> {
    fill_matches(plan, new, matching);
    for at in 0..old.declarations() {
        let old_at = DeclIx::new(at);
        let mut converted = conversion(plan, old_at);
        let Some(new_at) = converted.new else {
            continue;
        };
        let a = old.declaration(old_at).shape;
        let b = new.declaration(new_at).shape;
        converted.stops = converted.site.is_some();
        if !same_shape(a, b) {
            *plan.declarations.get_mut(at).expect("old declaration slot") = converted;
            continue;
        }
        let (old_first, old_count) = parts(a);
        let (new_first, new_count) = parts(b);
        converted.copies = old_count == new_count && converted.site.is_none();
        converted.derived = true;
        match a {
            Shape::Record { versioned: av, .. } => match b {
                Shape::Record { versioned: bv, .. } => {
                    if !identity && (av || bv) {
                        converted.copies = false;
                    }
                }
                Shape::Enum { .. } => unreachable!("same shape"),
            },
            Shape::Enum { .. } => {}
        }
        for offset in 0..new_count {
            let new_part = target(b, add(new_first, offset));
            let Some(old_part) = matching.part(new_part) else {
                converted.copies = false;
                match new_part {
                    Target::Field(field) => {
                        let source = *plan.sources.get(field.index()).expect("new field source");
                        match source {
                            Source::Written => converted.derived = false,
                            Source::Site(_) => {
                                converted.derived = false;
                                converted.stops = true;
                            }
                            Source::Value { .. } | Source::Old(_) => {}
                        }
                    }
                    Target::Variant(_) => {}
                    Target::Declaration(_) => unreachable!("part target"),
                }
                continue;
            };
            let expected = target(a, add(old_first, offset.min(old_count.saturating_sub(1))));
            converted.copies &= offset < old_count && old_part == expected;
            match new_part {
                Target::Field(new_field) => match old_part {
                    Target::Field(old_field) => {
                        let old_ty = old.field(old_field).ty;
                        let new_ty = new.field(new_field).ty;
                        let derived = compare(&matching.declarations, old, new, old_ty, new_ty) != Comparison::Retype;
                        *plan.fields.get_mut(old_field.index()).expect("old field flag") = derived;
                        converted.derived &= derived;
                        let site = site_for(plan, new_part);
                        if site.is_some() {
                            converted.stops = true;
                            converted.copies = false;
                        } else if converted.site.is_none() {
                            let (copies, stops) = reference_flags(plan, old, new, matching, old_ty, new_ty)?;
                            converted.copies &= copies;
                            converted.stops |= stops;
                        }
                    }
                    Target::Declaration(_) | Target::Variant(_) => unreachable!("same shape"),
                },
                Target::Variant(new_variant) => match old_part {
                    Target::Variant(old_variant) => {
                        let av = old.variant(old_variant).record;
                        let bv = new.variant(new_variant).record;
                        let derived = payload(&matching.declarations, av, bv);
                        *plan.variant_derived.get_mut(old_variant.index()).expect("old variant flag") = derived;
                        converted.copies &= derived;
                        if derived
                            && converted.site.is_none()
                            && let Some(record) = av
                        {
                            let reference = conversion(plan, record);
                            converted.copies &= reference.copies;
                            converted.stops |= reference.stops;
                        }
                    }
                    Target::Declaration(_) | Target::Field(_) => unreachable!("same shape"),
                },
                Target::Declaration(_) => unreachable!("part target"),
            }
        }
        *plan.declarations.get_mut(at).expect("old declaration slot") = converted;
    }
    Ok(())
}

pub(super) fn identity(tables: &Tables, index: u32) -> Result<Plan, PatchWrong> {
    let schema = tables.schemas.get(index).expect("kept schema index");
    let mut matching = empty_matching(schema);
    for at in 0..schema.declarations() {
        matching.pair(Target::Declaration(DeclIx::new(at)), Target::Declaration(DeclIx::new(at)));
    }
    for at in 0..matching.fields.len() {
        matching.pair(Target::Field(FieldIx::new(at)), Target::Field(FieldIx::new(at)));
    }
    for at in 0..matching.variants.len() {
        matching.pair(Target::Variant(VariantIx::new(at)), Target::Variant(VariantIx::new(at)));
    }
    let mut plan = empty_plan(schema, schema, index, index, 0, 0);
    fill(&mut plan, schema, schema, &matching, true)?;
    Ok(plan)
}
