use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use super::ProbeError;
use crate::guard::{GuardError, ScratchRoot};
use crate::source::ReadOnlySource;

/// A Navo flag that takes one path inside the run root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathFlag {
    Profile,
    Metrics,
    Trace,
    Left,
    Right,
    Folder,
    Out,
}

impl PathFlag {
    fn flag(self) -> &'static str {
        match self {
            Self::Profile => "--profile",
            Self::Metrics => "--metrics",
            Self::Trace => "--trace",
            Self::Left => "--left",
            Self::Right => "--right",
            Self::Folder => "--folder",
            Self::Out => "--out",
        }
    }

    /// Flags that only read their folder; the rest name files Navo writes.
    fn reads(self) -> bool {
        matches!(self, Self::Left | Self::Right | Self::Folder)
    }
}

/// One command-line item. Paths are relative to the run root and pass the guard.
#[derive(Clone, Copy, Debug)]
pub enum Arg<'a> {
    /// A fixed flag such as `--no-retention`: `--` then lowercase ASCII letters and dashes.
    Switch(&'static str),
    Path(PathFlag, &'a Path),
    /// The `bench` sub-command; only valid as the first item.
    Bench,
    /// An absolute folder outside the run root that Navo only reads: `Left`, `Right` or `Folder`.
    /// It must pass `ReadOnlySource::open`; a link, a system folder or the exe folder is refused.
    ReadOnly(PathFlag, &'a Path),
}

/// Builds the argument list; every path is checked by the run root's guard first, so the
/// caller builds it right before the process starts.
///
/// Residual: Navo opens these paths itself, so a folder swapped for a junction after the last
/// check can still redirect its writes. Checking here, again just before the start, and by
/// `verify_outputs` after the exit narrows that window but cannot close it.
pub(super) fn build(root: &ScratchRoot, args: &[Arg<'_>]) -> Result<Vec<OsString>, ProbeError> {
    let mut out = Vec::with_capacity(args.len() * 2);
    for arg in args {
        match *arg {
            Arg::Switch(switch) => {
                if !valid_switch(switch) {
                    return Err(ProbeError::BadSwitch);
                }
                out.push(OsString::from(switch));
            }
            Arg::Path(flag, rel) => {
                out.push(OsString::from(flag.flag()));
                out.push(inside(root, rel)?.into_os_string());
            }
            Arg::Bench if out.is_empty() => out.push(OsString::from("bench")),
            Arg::Bench => return Err(ProbeError::BadReadOnly),
            Arg::ReadOnly(flag, path) => {
                out.push(OsString::from(flag.flag()));
                out.push(outside(root, flag, path)?.into_os_string());
            }
        }
    }
    Ok(out)
}

fn inside(root: &ScratchRoot, rel: &Path) -> Result<PathBuf, ProbeError> {
    let plain = rel.components().count() > 0
        && rel
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if !plain {
        return Err(ProbeError::NotRelative);
    }
    let full = root.path().join(rel);
    root.check(&full).map_err(ProbeError::Guard)?;
    Ok(full)
}

/// Accepts a folder only when the guard says it is outside the run root and the read-only
/// source policy (system folders, exe folder, links, not a folder) accepts it.
fn outside(root: &ScratchRoot, flag: PathFlag, path: &Path) -> Result<PathBuf, ProbeError> {
    let plain = path.is_absolute()
        && path
            .components()
            .all(|part| !matches!(part, Component::ParentDir | Component::CurDir));
    if !flag.reads() || !plain {
        return Err(ProbeError::BadReadOnly);
    }
    // Any other guard error, such as a link on the path, rejects the folder.
    if !matches!(root.check(path), Err(GuardError::OutsideRoot(_))) {
        return Err(ProbeError::BadReadOnly);
    }
    ReadOnlySource::open(path, root.path()).map_err(|_| ProbeError::BadReadOnly)?;
    Ok(path.to_path_buf())
}

/// After the child exits: each file it was told to write must be a regular file at its place
/// inside the run root, which also shows it did not write elsewhere.
pub(crate) fn verify_outputs(root: &ScratchRoot, outputs: &[&Path]) -> Result<(), ProbeError> {
    for rel in outputs {
        let full = inside(root, rel)?;
        let found = std::fs::symlink_metadata(&full).map_err(|_| ProbeError::OutputMissing)?;
        if !found.is_file() {
            return Err(ProbeError::OutputMissing);
        }
    }
    Ok(())
}

pub(super) fn valid_switch(switch: &str) -> bool {
    switch.strip_prefix("--").is_some_and(|name| {
        !name.is_empty()
            && !name.starts_with('-')
            && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
    })
}
