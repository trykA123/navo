//! IW7 placement: whether a fresh install folder can be created safely.

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use crate::syntax::FolderProblem;

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;

/// Volume serial plus the 128-bit file id that identifies one folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileId {
    pub volume: u64,
    pub id: u128,
}

/// One existing folder between the drive root and the candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub path: PathBuf,
    pub name: OsString,
    pub id: FileId,
    pub reparse: bool,
}

/// The existing ancestors of a candidate and the names Setup would create below them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    pub existing: Vec<Link>,
    pub missing: Vec<OsString>,
}

/// A refused folder, identified by its anchor and the missing names below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protected {
    pub anchor: FileId,
    pub anchor_case_sensitive: Option<bool>,
    pub tail: Vec<OsString>,
}

/// The IW8a refused folders, grouped by how they compare with the candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refused {
    pub never_inside: Vec<Protected>,
    pub never_equal: Vec<Protected>,
    pub fresh_only: Vec<Protected>,
}

/// The drive kind returned by `GetDriveTypeW`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Remote,
    Other,
}

/// The drive facts read by W7b, before the candidate is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub drive: DriveKind,
    pub subst: bool,
    /// Whether another user can change an existing folder above the candidate.
    pub parent_open: bool,
    pub writable: bool,
    pub free: u64,
}

/// The sizes Velopack's space check needs, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    /// The embedded Setup.exe size; IW7's 4× floor uses it.
    pub payload: u64,
    /// The nupkg's compressed size.
    pub compressed: u64,
    /// The nupkg's extracted size.
    pub extracted: u64,
}

impl Space {
    /// Velopack's requirement: compressed + extracted + 50 MB overhead
    /// (`velopack/src/bins/src/commands/install.rs:53-54`).
    pub fn velopack_requirement(&self) -> u64 {
        self.compressed
            .saturating_add(self.extracted)
            .saturating_add(SETUP_OVERHEAD)
    }
}

/// Velopack's fixed setup overhead, in bytes (`install.rs:54`).
const SETUP_OVERHEAD: u64 = 50 * 1000 * 1000;

/// Compares two folder names the way Windows does.
pub trait NameEq {
    /// Whether `a` and `b` name the same folder.
    fn same(&self, a: &OsStr, b: &OsStr) -> bool;
}

/// Checks a fresh folder against IW7 and IW8a; every unknown fact refuses.
///
/// `space` carries the Setup.exe size and the nupkg sizes Velopack checks.
#[expect(
    clippy::too_many_arguments,
    reason = "the chain, refused sets, facts, space inputs and name equality are all inputs"
)]
pub fn check(
    chain: &Chain,
    refused: &Refused,
    facts: &Facts,
    space: &Space,
    names: &dyn NameEq,
) -> Result<(), FolderProblem> {
    if chain.existing.is_empty() || chain.missing.iter().any(|name| name.is_empty()) {
        return Err(FolderProblem::ProbeFailed);
    }
    if chain.missing.is_empty() {
        return Err(FolderProblem::RootExists);
    }
    if chain.existing.iter().any(|link| link.reparse) {
        return Err(FolderProblem::ReparsePoint);
    }
    if facts.subst {
        return Err(FolderProblem::SubstDrive);
    }
    if facts.parent_open {
        return Err(FolderProblem::ParentOpen);
    }
    if !matches!(facts.drive, DriveKind::Fixed | DriveKind::Removable) {
        return Err(FolderProblem::DriveNotLocal);
    }
    if !facts.writable {
        return Err(FolderProblem::NotWritable);
    }
    let required = space
        .payload
        .saturating_mul(4)
        .max(space.velopack_requirement());
    if facts.free < required {
        return Err(FolderProblem::NotEnoughSpace);
    }
    if refused
        .never_inside
        .iter()
        .any(|folder| inside(chain, folder, names))
        || refused
            .never_equal
            .iter()
            .any(|folder| equal(chain, folder, names))
        || refused
            .fresh_only
            .iter()
            .any(|folder| inside(chain, folder, names))
    {
        return Err(FolderProblem::ProtectedFolder);
    }
    Ok(())
}

/// Whether the candidate is at or below `protected`.
///
/// A case-sensitive anchor, or one whose case sensitivity is unknown, returns true for every
/// candidate whose chain holds it, so the caller refuses (IW8a).
pub fn inside(chain: &Chain, protected: &Protected, names: &dyn NameEq) -> bool {
    let Some(index) = anchor_index(chain, protected.anchor) else {
        return false;
    };
    if protected.anchor_case_sensitive != Some(false) {
        return true;
    }
    has_prefix(chain, index, &protected.tail, names)
}

/// Whether two paths name the same place component by component under `names`.
pub fn same_path(a: &Path, b: &Path, names: &impl NameEq) -> bool {
    if !a.is_absolute() || !b.is_absolute() {
        return false;
    }
    let left: Vec<Component<'_>> = a.components().collect();
    let right: Vec<Component<'_>> = b.components().collect();
    if left.len() != right.len()
        || left
            .iter()
            .chain(&right)
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    left.iter()
        .zip(&right)
        .all(|(left, right)| same_component(left, right, names))
}

/// Compares one path component, case-insensitively where the name rule says so.
fn same_component(left: &Component<'_>, right: &Component<'_>, names: &impl NameEq) -> bool {
    match (left, right) {
        (Component::Prefix(left), Component::Prefix(right)) => {
            names.same(left.as_os_str(), right.as_os_str())
        }
        (Component::RootDir, Component::RootDir) => true,
        (Component::Normal(left), Component::Normal(right)) => names.same(left, right),
        _ => false,
    }
}

/// Returns the new `Navo` folder inside the parent picked in Browse.
pub fn browse_target(picked: &Path) -> PathBuf {
    let picked = picked.to_string_lossy().replace('/', "\\");
    let parent = picked.trim_end_matches('\\');
    PathBuf::from(format!("{parent}\\Navo"))
}

/// Whether the candidate is exactly `protected`.
fn equal(chain: &Chain, protected: &Protected, names: &dyn NameEq) -> bool {
    let Some(index) = anchor_index(chain, protected.anchor) else {
        return false;
    };
    if protected.anchor_case_sensitive != Some(false) {
        return true;
    }
    let mut below = names_below(chain, index);
    let same = protected.tail.iter().all(|expected| {
        below
            .next()
            .is_some_and(|name| names.same(name, expected.as_os_str()))
    });
    same && below.next().is_none()
}

/// Whether `tail` is a prefix of the candidate's names below `index`.
fn has_prefix(chain: &Chain, index: usize, tail: &[OsString], names: &dyn NameEq) -> bool {
    let mut below = names_below(chain, index);
    tail.iter().all(|expected| {
        below
            .next()
            .is_some_and(|name| names.same(name, expected.as_os_str()))
    })
}

/// The candidate's names below the existing link at `index`.
fn names_below(chain: &Chain, index: usize) -> impl Iterator<Item = &OsStr> {
    chain.existing[index + 1..]
        .iter()
        .map(|link| link.name.as_os_str())
        .chain(chain.missing.iter().map(OsString::as_os_str))
}

/// The position of the anchor in the candidate's existing chain.
fn anchor_index(chain: &Chain, anchor: FileId) -> Option<usize> {
    chain.existing.iter().position(|link| link.id == anchor)
}
