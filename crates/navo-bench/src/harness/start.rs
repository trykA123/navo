use std::fmt;
use std::path::{Path, PathBuf};

use super::args::Args;
use super::picker::{PickerError, pick_folder};
use crate::privacy::{ErrorCode, PathRole, PrivacyError};
use crate::source::{ReadOnlySource, SourceError};

/// Why no folder could be opened. Never holds a path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
    /// No picker on this OS and no `--folder`.
    PathRequired,
    Cancelled,
    Picker(PickerError),
    Refused(PrivacyError),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathRequired => f.write_str("start:folder-argument-required"),
            Self::Cancelled => f.write_str("start:cancelled"),
            Self::Picker(error) => error.fmt(f),
            Self::Refused(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for StartError {}

/// Opens the folder from `--folder`, or from the picker when there is no flag.
pub fn open_owner_folder(args: &Args, scratch_root: &Path) -> Result<ReadOnlySource, StartError> {
    open_with(args, scratch_root, pick_folder)
}

/// The folder from `--folder`, or from `picker` when there is no flag; `Ok(None)` means cancelled.
pub fn choose_folder(
    args: &Args,
    picker: impl FnOnce() -> Result<Option<PathBuf>, PickerError>,
) -> Result<Option<PathBuf>, StartError> {
    match &args.folder {
        Some(folder) => Ok(Some(folder.clone())),
        None => match picker() {
            Ok(folder) => Ok(folder),
            Err(PickerError::Unsupported) => Err(StartError::PathRequired),
            Err(error) => Err(StartError::Picker(error)),
        },
    }
}

/// Opens a chosen folder read-only; refuses system folders, the exe folder and the run root.
pub fn open_folder(folder: &Path, scratch_root: &Path) -> Result<ReadOnlySource, StartError> {
    ReadOnlySource::open(folder, scratch_root).map_err(refusal)
}

pub(super) fn open_with(
    args: &Args,
    scratch_root: &Path,
    picker: impl FnOnce() -> Result<Option<PathBuf>, PickerError>,
) -> Result<ReadOnlySource, StartError> {
    let folder = choose_folder(args, picker)?.ok_or(StartError::Cancelled)?;
    open_folder(&folder, scratch_root)
}

fn refusal(error: SourceError) -> StartError {
    let role = match error {
        SourceError::ScratchRoot => PathRole::ScratchRoot,
        _ => PathRole::OwnerFolder,
    };
    let code = match error {
        SourceError::Unavailable(kind) => ErrorCode::Io(kind),
        other => ErrorCode::Refused(tag(other)),
    };
    StartError::Refused(PrivacyError { role, code })
}

fn tag(error: SourceError) -> &'static str {
    match error {
        SourceError::DriveRoot => "drive-root",
        SourceError::SystemDriveRoot => "system-drive-root",
        SourceError::WindowsFolder => "windows-folder",
        SourceError::ProgramFiles => "program-files",
        SourceError::ProgramFilesX86 => "program-files-x86",
        SourceError::ProgramData => "program-data",
        SourceError::ExeFolder => "exe-folder",
        SourceError::ScratchRoot => "scratch-root",
        SourceError::UserProfileRoot => "user-profile-root",
        SourceError::UncServerRoot => "unc-server-root",
        SourceError::FolderLookup => "folder-lookup",
        SourceError::Unavailable(_) => "unavailable",
        SourceError::NotDirectory => "not-directory",
        SourceError::Link => "link",
        SourceError::ForeignId => "foreign-id",
        SourceError::NotFile => "not-file",
    }
}
