//! Changes between kept and current schema tables (codec.md, section 10).

use core::fmt;
use skein_codec::schema::Schema;

mod compare;
mod matching;
mod walk;

/// A family's changes and unconfirmed renames (codec.md, 10.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    pub family: String,
    pub kept: u16,
    pub current: u16,
    pub changes: Vec<Change>,
    pub proposed: Vec<Rename>,
}

/// One operation and the conversion it needs (codec.md, 10.2–10.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub kind: Kind,
    pub op: Op,
}

/// Conversion requirements, in increasing order (codec.md, 10.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    NoneNeeded,
    Derived,
    Written,
}

/// The one-line operations of codec.md, 10.2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    AddDeclaration { decl: String },
    DropDeclaration { decl: String },
    RenameDeclaration { old: String, new: String },
    Reshape { decl: String },
    Versioned { decl: String, versioned: bool },
    AddField { decl: String, field: String, ty: String, value: Fill },
    DropField { decl: String, field: String },
    RenamePart { decl: String, old: String, new: String },
    Widen { decl: String, field: String, kept: String, current: String },
    Wrap { decl: String, field: String, kept: String, current: String },
    Retype { decl: String, field: String, kept: String, current: String },
    Order { decl: String, parts: Vec<String> },
    AddVariant { decl: String, variant: String },
    DropVariant { decl: String, variant: String },
    Payload { decl: String, variant: String, kept: Option<String>, current: Option<String> },
}

/// An added field's value or a request for one (codec.md, 10.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    Absent,
    Asked,
}

/// A confirmed or proposed pairing; `decl` is current (codec.md, 10.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rename {
    Declaration { old: String, new: String },
    Part { decl: String, old: String, new: String },
}

/// Whether the current header follows its changes (codec.md, 10.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    Kept,
    Moved,
    Unmoved,
    Needless,
    Wrong,
}

/// Different families or the first invalid rename's index (codec.md, 10.1, 10.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffError {
    Family,
    Rename(usize),
}

/// Compares two parsed versions of one family (codec.md, 10.1–10.4).
pub fn diff(kept: &Schema, current: &Schema, renames: &[Rename]) -> Result<Diff, DiffError> {
    if kept.spelled(kept.family()) != current.spelled(current.family()) {
        return Err(DiffError::Family);
    }
    let pairs = matching::declarations(kept, current, renames)?;
    Ok(walk::diff(kept, current, renames, &pairs))
}

impl Diff {
    /// The greatest requirement, absent with no changes (codec.md, 10.3).
    #[must_use]
    pub fn kind(&self) -> Option<Kind> {
        self.changes.iter().map(|change| change.kind).max()
    }

    /// Checks the header without overflowing the kept version (codec.md, 10.4).
    #[must_use]
    pub fn version(&self) -> Version {
        let needs = self.kind() >= Some(Kind::Derived);
        if self.current == self.kept {
            if needs { Version::Unmoved } else { Version::Kept }
        } else if self.kept.checked_add(1) == Some(self.current) {
            if needs { Version::Moved } else { Version::Needless }
        } else {
            Version::Wrong
        }
    }

    /// Distinct written sites in change order (codec.md, 10.3).
    #[must_use]
    pub fn written(&self) -> Vec<String> {
        let mut sites = Vec::new();
        for change in &self.changes {
            if change.kind == Kind::Written {
                let site = change.op.declaration();
                if !sites.iter().any(|name| name == site) {
                    sites.push(site.to_owned());
                }
            }
        }
        sites
    }
}

impl Op {
    fn declaration(&self) -> &str {
        match self {
            Self::RenameDeclaration { new, .. } => new,
            Self::AddDeclaration { decl }
            | Self::DropDeclaration { decl }
            | Self::Reshape { decl }
            | Self::Versioned { decl, .. }
            | Self::AddField { decl, .. }
            | Self::DropField { decl, .. }
            | Self::RenamePart { decl, .. }
            | Self::Widen { decl, .. }
            | Self::Wrap { decl, .. }
            | Self::Retype { decl, .. }
            | Self::Order { decl, .. }
            | Self::AddVariant { decl, .. }
            | Self::DropVariant { decl, .. }
            | Self::Payload { decl, .. } => decl,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoneNeeded => "none",
            Self::Derived => "derived",
            Self::Written => "written",
        })
    }
}

impl fmt::Display for Fill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Absent => "none",
            Self::Asked => "?",
        })
    }
}

impl fmt::Display for Rename {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declaration { old, new } => write!(f, "rename {old} {new}"),
            Self::Part { decl, old, new } => write!(f, "rename {decl}.{old} {new}"),
        }
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddDeclaration { decl } => write!(f, "add {decl}"),
            Self::DropDeclaration { decl } => write!(f, "drop {decl}"),
            Self::RenameDeclaration { old, new } => write!(f, "rename {old} {new}"),
            Self::Reshape { decl } => write!(f, "reshape {decl}"),
            Self::Versioned { decl, versioned } => {
                let word = if *versioned { "versioned" } else { "unversioned" };
                write!(f, "{word} {decl}")
            }
            Self::AddField { decl, field, ty, value } => write!(f, "add {decl}.{field} {ty} {value}"),
            Self::DropField { decl, field } => write!(f, "drop {decl}.{field}"),
            Self::RenamePart { decl, old, new } => write!(f, "rename {decl}.{old} {new}"),
            Self::Widen { decl, field, kept, current } => write!(f, "widen {decl}.{field} {kept} {current}"),
            Self::Wrap { decl, field, kept, current } => write!(f, "wrap {decl}.{field} {kept} {current}"),
            Self::Retype { decl, field, kept, current } => write!(f, "retype {decl}.{field} {kept} {current}"),
            Self::Order { decl, parts } => write!(f, "order {decl} {}", parts.join(" ")),
            Self::AddVariant { decl, variant } => write!(f, "add {decl}.{variant}"),
            Self::DropVariant { decl, variant } => write!(f, "drop {decl}.{variant}"),
            Self::Payload { decl, variant, kept, current } => {
                write!(
                    f,
                    "payload {decl}.{variant} {} {}",
                    kept.as_deref().unwrap_or("-"),
                    current.as_deref().unwrap_or("-")
                )
            }
        }
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.op.fmt(f)
    }
}

impl fmt::Display for Diff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "family {} {} to {}", self.family, self.kept, self.current)?;
        for change in &self.changes {
            writeln!(f, "{} {change}", change.kind)?;
        }
        for rename in &self.proposed {
            writeln!(f, "proposed {rename}")?;
        }
        Ok(())
    }
}
