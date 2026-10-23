//! Level-by-level comparison and grammar spelling (codec.md, 10.3 and 7.1).

use super::Kind;
use super::matching::{Pairs, name, word};
use skein_codec::schema::{DeclIx, Held, Schema, Type, TypeIx};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Comparison {
    Equal,
    Widen(Kind),
    Wrap,
    Retype,
}

pub(super) struct Types<'a> {
    pub kept: &'a Schema,
    pub current: &'a Schema,
    pub pairs: &'a Pairs,
}

impl Types<'_> {
    pub fn payload(&self, kept: Option<DeclIx>, current: Option<DeclIx>) -> bool {
        match (kept, current) {
            (None, None) => true,
            (Some(k), Some(c)) => self.pairs.get(&c.index()) == Some(&k.index()),
            (None, Some(_)) | (Some(_), None) => false,
        }
    }

    fn held(&self, kept: Held, current: Held) -> Comparison {
        match (kept, current) {
            (Held::Opaque, Held::Opaque) => Comparison::Equal,
            (Held::Opaque, Held::Record { .. } | Held::Pieces(_))
            | (Held::Record { .. } | Held::Pieces(_), Held::Opaque) => Comparison::Widen(Kind::NoneNeeded),
            (Held::Record { family: kf, name: kn }, Held::Record { family: cf, name: cn }) => {
                if self.kept.spelled(kf) == self.current.spelled(cf)
                    && self.kept.spelled(kn) == self.current.spelled(cn)
                {
                    Comparison::Equal
                } else {
                    Comparison::Retype
                }
            }
            (Held::Pieces(k), Held::Pieces(c)) => {
                if self.payload(Some(k), Some(c)) {
                    Comparison::Equal
                } else {
                    Comparison::Retype
                }
            }
            (Held::Record { .. }, Held::Pieces(_)) | (Held::Pieces(_), Held::Record { .. }) => Comparison::Retype,
        }
    }

    pub fn compare(&self, mut kept: TypeIx, mut current: TypeIx) -> Comparison {
        let mut widening = false;
        let mut integer = false;
        let mut wrapped = false;
        loop {
            let k = self.kept.ty(kept);
            let c = self.current.ty(current);
            if let Type::Option(inner) = c {
                current = inner;
                if let Type::Option(inner) = k {
                    kept = inner;
                } else {
                    wrapped = true;
                }
                continue;
            }
            if let (Type::List { bound: kb, item: ki }, Type::List { bound: cb, item: ci }) = (k, c) {
                if cb < kb {
                    return Comparison::Retype;
                }
                widening |= cb > kb;
                kept = ki;
                current = ci;
                continue;
            }
            let leaf =
                if k == c && !matches!(k, Type::Named(_) | Type::Bytes { .. } | Type::List { .. } | Type::Option(_)) {
                    Comparison::Equal
                } else if let (Some(k), Some(c)) = (integer_width(k), integer_width(c)) {
                    if c > k { Comparison::Widen(Kind::Derived) } else { Comparison::Retype }
                } else {
                    match (k, c) {
                        (Type::Named(k), Type::Named(c)) => {
                            if self.payload(Some(k), Some(c)) {
                                Comparison::Equal
                            } else {
                                Comparison::Retype
                            }
                        }
                        (Type::Text(k), Type::Text(c) | Type::Secret(c) | Type::Bytes { bound: c, .. })
                        | (Type::Secret(k) | Type::Bytes { bound: k, .. }, Type::Secret(c))
                        | (Type::Secret(k), Type::Bytes { bound: c, .. }) => {
                            if c >= k {
                                Comparison::Widen(Kind::NoneNeeded)
                            } else {
                                Comparison::Retype
                            }
                        }
                        (Type::Bytes { bound: k, held: kh }, Type::Bytes { bound: c, held: ch }) => {
                            if c < k {
                                Comparison::Retype
                            } else {
                                match self.held(kh, ch) {
                                    Comparison::Equal if c > k => Comparison::Widen(Kind::NoneNeeded),
                                    result @ (Comparison::Equal
                                    | Comparison::Widen(_)
                                    | Comparison::Wrap
                                    | Comparison::Retype) => result,
                                }
                            }
                        }
                        _ => Comparison::Retype,
                    }
                };
            match leaf {
                Comparison::Retype => return Comparison::Retype,
                Comparison::Equal => {}
                Comparison::Widen(kind) => {
                    widening = true;
                    integer |= kind == Kind::Derived;
                }
                Comparison::Wrap => unreachable!("leaf comparisons do not wrap"),
            }
            return if wrapped {
                Comparison::Wrap
            } else if integer {
                Comparison::Widen(Kind::Derived)
            } else if widening {
                Comparison::Widen(Kind::NoneNeeded)
            } else {
                Comparison::Equal
            };
        }
    }
}

fn integer_width(ty: Type) -> Option<u8> {
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

pub(super) fn spell(schema: &Schema, mut at: TypeIx) -> String {
    let mut tokens = Vec::new();
    loop {
        let leaf = match schema.ty(at) {
            Type::List { bound, item } => {
                tokens.push(format!("list {bound}"));
                at = item;
                continue;
            }
            Type::Option(item) => {
                tokens.push("option".into());
                at = item;
                continue;
            }
            Type::U8 => "u8".into(),
            Type::U16 => "u16".into(),
            Type::U32 => "u32".into(),
            Type::U64 => "u64".into(),
            Type::Bool => "bool".into(),
            Type::Duration => "duration".into(),
            Type::Fixed(n) => format!("fixed {n}"),
            Type::Secret(n) => format!("secret {n}"),
            Type::Text(n) => format!("text {n}"),
            Type::Named(at) => name(schema, at.index()),
            Type::Bytes { bound, held } => match held {
                Held::Opaque => format!("bytes {bound}"),
                Held::Record { family, name } => {
                    format!("bytes {bound} holds {} {}", word(schema, family), word(schema, name))
                }
                Held::Pieces(at) => format!("bytes {bound} pieces {}", name(schema, at.index())),
            },
        };
        tokens.push(leaf);
        return tokens.join(" ");
    }
}
