//! The patch command and its check over an owner's crate (codec.md, section 12).
//! Ordinary Rust: it reads and writes the crate's files, and reads no git. What
//! is landed is its input (12.8), read by the binary.

mod check;
mod derive;
mod files;
mod golden;
mod module;
mod renumber;
mod run;
mod skeleton;
mod text;

pub use check::check;
pub use golden::{Mode, Written, golden, goldens};
pub use module::{current, module};
pub use run::run;

use alloc::collections::BTreeMap;
use core::fmt;
use std::path::{Path, PathBuf};

use crate::diff::{DiffError, Rename, Version};

/// What the command is told beyond its crate (codec.md, 12.8).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `--owner`: the header's owner, for a crate's first patch.
    pub owner: Option<String>,
    /// `--after <upstream> <level>`, in the order given.
    pub after: Vec<(String, u32)>,
    /// `--rename <family> <old> <new>`: a family and a rename its author confirms.
    pub renames: Vec<(String, Rename)>,
    /// `--baseline`: the crate's first run, at its first release (12.1).
    pub baseline: bool,
}

/// The base's files under the crate's `patches/` and `schema/kept/`, by path
/// relative to the crate (codec.md, 12).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Landed {
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl Landed {
    /// The landed files, by path relative to the crate.
    #[must_use]
    pub const fn new(files: BTreeMap<PathBuf, Vec<u8>>) -> Self {
        Self { files }
    }

    /// The tree's own files under `patches/` and `schema/kept/`, as if landed.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] for a directory or file that cannot be read.
    pub fn read(owner: &Path) -> Result<Self, Error> {
        files::landed(owner).map(Self::new)
    }

    /// Nothing landed: a crate before its baseline.
    #[must_use]
    pub const fn none() -> Self {
        Self { files: BTreeMap::new() }
    }

    /// A landed file's bytes.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    /// Every landed path, in order.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }
}

/// What a run did (codec.md, 12.2 and 12.8).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Each family's, in name order.
    pub outcomes: Vec<Outcome>,
    /// The branch's patch, written or derived again.
    pub patch: Option<u32>,
    /// The number the branch's patch was moved from (12.6).
    pub moved: Option<u32>,
    /// Renames proposed and not confirmed, with their families.
    pub proposed: Vec<(String, Rename)>,
    /// What its author must give: each `?` line, each site to write.
    pub asked: Vec<String>,
    /// Each landed file the tree lacked or held otherwise, written back from
    /// the base (12.2, 6), relative to the crate.
    pub restored: Vec<PathBuf>,
    /// The check's, after the run (12.7).
    pub findings: Vec<Finding>,
}

/// A family's outcome in a run (codec.md, 12.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Its first kept text, the baseline's or the family's.
    Kept { family: String, version: u16 },
    /// Nothing to do.
    Current { family: String },
    /// Its latest kept text made a copy of the current one.
    Refreshed { family: String, version: u16 },
    /// Renames waiting for the family's next patch (12.4).
    Waiting { family: String, renames: Vec<Rename> },
    /// A block in the branch's patch.
    Patched { family: String, from: u16, to: u16 },
    /// `Unmoved`, `Needless` or `Wrong`: nothing written for the family (12.5).
    Refused { family: String, verdict: Version },
}

/// What the check finds (codec.md, 12.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    /// A current text with no kept text.
    Unkept { family: String },
    /// A kept version missing between a family's first and its latest.
    KeptGap { family: String, missing: u16 },
    /// A version no patch moves the family to, kept or current.
    Unpatched { family: String, version: u16 },
    /// A latest kept text not frozen that differs from the current with no
    /// patch needed.
    Lagging { family: String, version: u16 },
    /// `Unmoved`, `Needless` or `Wrong` (12.5).
    Version { family: String, verdict: Version, kept: u16, current: u16 },
    /// A landed patch's text or goldens, or a landed kept text, changed or gone.
    Changed { path: PathBuf },
    /// A patch number missing between 1 and the last.
    PatchGap { missing: u32 },
    /// A patch whose header's level is not its number.
    Misnumbered { patch: u32, level: u32 },
    /// A block that does not plan against its kept texts (11.3), the
    /// planner's refusal spelled.
    Unplanned { patch: u32, family: String, error: String },
}

/// Why the command, or one of its parts, stops (codec.md, 12.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A file or directory that cannot be read or written.
    Io { path: PathBuf, error: String },
    /// A text that does not parse, or a kept text named otherwise than its
    /// header says.
    Schema { path: PathBuf, error: String },
    /// The diff's refusal of a family's texts or renames.
    Diff { family: String, error: DiffError },
    /// A crate with no kept text, landed or its own, run without
    /// `--baseline` (12.1).
    Baseline,
    /// A crate's first patch without `--owner`.
    Owner,
    /// Two patches of the branch's own (12, one patch per landing).
    Unlanded { patches: Vec<u32> },
    /// A file holding conflict markers (12.6).
    Conflict { path: PathBuf },
    /// Two names that spell one constant or function (12.3).
    Spelled { first: String, second: String },
    /// Each golden that fails, named (12.3).
    Golden(Vec<String>),
    /// `patches/mod.rs` not as generated (12.3).
    Stale,
}

impl Error {
    pub(crate) fn io(path: &Path, error: &std::io::Error) -> Self {
        Self::Io { path: path.to_path_buf(), error: error.to_string() }
    }
}

/// A verdict's name, as codec.md 10.4 spells it.
const fn verdict_name(verdict: Version) -> &'static str {
    match verdict {
        Version::Kept => "Kept",
        Version::Moved => "Moved",
        Version::Unmoved => "Unmoved",
        Version::Needless => "Needless",
        Version::Wrong => "Wrong",
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kept { family, version } => write!(f, "{family}: kept version {version}, its first"),
            Self::Current { family } => write!(f, "{family}: current"),
            Self::Refreshed { family, version } => write!(f, "{family}: kept version {version} refreshed"),
            Self::Waiting { family, renames } => {
                write!(f, "{family}: renames waiting for its next patch:")?;
                for rename in renames {
                    write!(f, " {rename};")?;
                }
                Ok(())
            }
            Self::Patched { family, from, to } => write!(f, "{family}: patched {from} to {to}"),
            Self::Refused { family, verdict } => {
                write!(f, "{family}: refused, {} (codec.md, 12.5)", verdict_name(*verdict))
            }
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const RUN: &str = "run `skein-codegen patch <crate>`";
        match self {
            Self::Unkept { family } => write!(f, "{family}: no kept text; {RUN}"),
            Self::KeptGap { family, missing } => {
                write!(f, "{family}: kept version {missing} missing; kept versions run without a gap")
            }
            Self::Unpatched { family, version } => write!(f, "{family}: no patch moves it to version {version}; {RUN}"),
            Self::Lagging { family, version } => {
                write!(f, "{family}: kept version {version} lags the current text; {RUN}")
            }
            Self::Version { family, verdict, kept, current } => {
                write!(f, "{family}: version {current} over kept {kept} is {} (codec.md, 12.5)", verdict_name(*verdict))
            }
            Self::Changed { path } => write!(f, "{}: landed, and changed or gone", path.display()),
            Self::PatchGap { missing } => write!(f, "patch {missing:04} missing"),
            Self::Misnumbered { patch, level } => write!(f, "patch {patch:04}: its header's level is {level}"),
            Self::Unplanned { patch, family, error } => {
                write!(f, "patch {patch:04}, {family}: does not plan: {error}")
            }
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } | Self::Schema { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Diff { family, error: DiffError::Family } => write!(f, "{family}: two texts of different families"),
            Self::Diff { family, error: DiffError::Rename(index) } => {
                write!(f, "{family}: rename {index} refused (codec.md, 10.1)")
            }
            Self::Baseline => {
                f.write_str("the crate keeps no text yet: before its baseline; pass --baseline at its first release")
            }
            Self::Owner => f.write_str("the crate's first patch needs --owner <name>"),
            Self::Unlanded { patches } => {
                f.write_str("patches")?;
                for patch in patches {
                    write!(f, " {patch:04}")?;
                }
                f.write_str(" are all the branch's: one landing writes one patch")
            }
            Self::Conflict { path } => write!(
                f,
                "{}: conflict markers; take the branch's side first (git checkout --theirs -- <crate>/patches <crate>/schema/kept)",
                path.display()
            ),
            Self::Spelled { first, second } => write!(f, "{first} and {second} spell one name"),
            Self::Golden(failures) => {
                f.write_str("goldens fail:")?;
                for failure in failures {
                    write!(f, "\n  {failure}")?;
                }
                Ok(())
            }
            Self::Stale => f.write_str("patches/mod.rs is not as generated; run `skein-codegen patch <crate>`"),
        }
    }
}

impl core::error::Error for Error {}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for outcome in &self.outcomes {
            writeln!(f, "{outcome}")?;
        }
        if let Some(number) = self.patch {
            write!(f, "patch {number:04}")?;
            if let Some(old) = self.moved {
                write!(f, " (moved from {old:04})")?;
            }
            writeln!(f)?;
        }
        for path in &self.restored {
            writeln!(f, "restored {}", path.display())?;
        }
        for asked in &self.asked {
            writeln!(f, "asked: {asked}")?;
        }
        for (family, rename) in &self.proposed {
            match rename {
                Rename::Declaration { old, new } => writeln!(f, "--rename {family} {old} {new}")?,
                Rename::Part { decl, old, new } => writeln!(f, "--rename {family} {decl}.{old} {new}")?,
            }
        }
        for finding in &self.findings {
            writeln!(f, "{finding}")?;
        }
        Ok(())
    }
}
