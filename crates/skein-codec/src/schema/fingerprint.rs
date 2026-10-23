//! Canonical layout fingerprints (codec.md, section 9).

use super::{DeclIx, FieldIx, Held, Schema, Shape, Type, TypeIx, VariantIx, add};
use skein_lib::{List, sha256::Sha256};

/// A layout's SHA-256 (codec.md, section 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    /// The digest's bytes in SHA-256 order.
    #[must_use]
    pub const fn digest(self) -> [u8; 32] {
        self.0
    }
}

/// A fingerprint per declaration, in the text's order, and the family's.
#[derive(Debug)]
pub struct Fingerprints {
    declarations: List<Fingerprint>,
    family: Fingerprint,
}

impl Fingerprints {
    /// The fingerprint at an index belonging to the schema.
    #[must_use]
    pub fn declaration(&self, at: DeclIx) -> Fingerprint {
        *self.declarations.get(at.index()).expect("declaration index belongs to the schema")
    }

    /// The fingerprint of all declarations in text order.
    #[must_use]
    pub fn family(&self) -> Fingerprint {
        self.family
    }
}

impl Schema {
    /// Hashes the canonical encoding of each layout and the family (codec.md, 9.1).
    #[must_use]
    pub fn fingerprints(&self) -> Fingerprints {
        let mut declarations = List::with_capacity(self.declarations());
        let mut family = Sha256::new();
        family.update(&[1, 3]);
        family.update(&self.declarations().to_be_bytes());
        for at in 0..self.declarations() {
            let mut hash = Sha256::new();
            hash.update(&[1]);
            match self.declaration(DeclIx::new(at)).shape {
                Shape::Record { versioned, first, count } => {
                    hash.update(&[1, u8::from(versioned)]);
                    if versioned {
                        hash.update(&self.version().to_be_bytes());
                    }
                    hash.update(&count.to_be_bytes());
                    for at in first.index()..add(first.index(), count) {
                        hash_type(self, &declarations, self.field(FieldIx::new(at)).ty, &mut hash);
                    }
                }
                Shape::Enum { first, count } => {
                    hash.update(&[2]);
                    hash.update(&count.to_be_bytes());
                    for at in first.index()..add(first.index(), count) {
                        match self.variant(VariantIx::new(at)).record {
                            None => hash.update(&[0]),
                            Some(record) => {
                                hash.update(&[1]);
                                hash_reference(&declarations, record, &mut hash);
                            }
                        }
                    }
                }
            }
            let fingerprint = Fingerprint(hash.finish());
            family.update(&fingerprint.digest());
            declarations.push(fingerprint).expect("one fingerprint per declaration");
        }
        Fingerprints { declarations, family: Fingerprint(family.finish()) }
    }
}

fn hash_reference(declarations: &List<Fingerprint>, at: DeclIx, hash: &mut Sha256) {
    hash.update(&declarations.get(at.index()).expect("referenced declaration is earlier").digest());
}

fn hash_name(name: &[u8], hash: &mut Sha256) {
    hash.update(&u32::try_from(name.len()).expect("identifier length is bounded by schema text").to_be_bytes());
    hash.update(name);
}

fn hash_type(schema: &Schema, declarations: &List<Fingerprint>, mut at: TypeIx, hash: &mut Sha256) {
    // Every wrapper advances to its item; the chain ends within the types table.
    for _ in 0..schema.types.len() {
        match schema.ty(at) {
            Type::U8 => hash.update(&[1]),
            Type::U16 => hash.update(&[2]),
            Type::U32 => hash.update(&[3]),
            Type::U64 => hash.update(&[4]),
            Type::Bool => hash.update(&[5]),
            Type::Duration => hash.update(&[6]),
            Type::Fixed(bound) => {
                hash.update(&[7]);
                hash.update(&bound.to_be_bytes());
            }
            Type::Secret(bound) => {
                hash.update(&[8]);
                hash.update(&bound.to_be_bytes());
            }
            Type::Text(bound) => {
                hash.update(&[9]);
                hash.update(&bound.to_be_bytes());
            }
            Type::Bytes { bound, held } => {
                hash.update(&[10]);
                hash.update(&bound.to_be_bytes());
                match held {
                    Held::Opaque => hash.update(&[0]),
                    Held::Record { family, name } => {
                        hash.update(&[1]);
                        hash_name(schema.spelled(family), hash);
                        hash_name(schema.spelled(name), hash);
                    }
                    Held::Pieces(target) => {
                        hash.update(&[2]);
                        hash_reference(declarations, target, hash);
                    }
                }
            }
            Type::List { bound, item } => {
                hash.update(&[11]);
                hash.update(&bound.to_be_bytes());
                at = item;
                continue;
            }
            Type::Option(value) => {
                hash.update(&[12]);
                at = value;
                continue;
            }
            Type::Named(target) => {
                hash.update(&[13]);
                hash_reference(declarations, target, hash);
            }
        }
        break;
    }
}

#[cfg(test)]
mod tests;
