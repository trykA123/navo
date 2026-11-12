//! IW6's Windows folder-path syntax: refuses unsafe paths before any file-system probe.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "syntax_tests.rs"]
mod tests;

/// IW6's path limit, in UTF-16 units.
const MAX_UNITS: usize = 150;

/// Device names Windows reserves in every folder, with or without an extension.
const RESERVED_NAMES: [&str; 30] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "CONIN$",
    "CONOUT$", "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³",
];

/// A Windows path that passed IW6. IW7's placement checks can still refuse it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate(PathBuf);

impl Candidate {
    /// The path exactly as the user gave it.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// One reason Setup refuses a folder. `message` says what is wrong; `fix` says what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderProblem {
    NotAbsolute,
    DotComponent,
    NetworkOrDevicePath,
    AlternateDataStream,
    InvalidCharacter,
    ReservedName,
    TrailingDotOrSpace,
    TooLong,
    DriveRoot,
    ShellMetacharacter(char),
    RootExists,
    ReparsePoint,
    ProtectedFolder,
    NeedsAdministrator,
    DriveNotLocal,
    SubstDrive,
    NotWritable,
    NotEnoughSpace,
    ProbeFailed,
}

impl FolderProblem {
    /// One user-facing line saying what is wrong with the folder.
    pub fn message(&self) -> &'static str {
        match self {
            Self::NotAbsolute => {
                "The folder path must start with a drive letter, a colon and a separator, such as C:\\Navo."
            }
            Self::DotComponent => {
                "The folder path contains a . or .. part, which Setup does not accept."
            }
            Self::NetworkOrDevicePath => "Setup cannot install to a UNC, extended or device path.",
            Self::AlternateDataStream => "The folder path has a colon after the drive letter.",
            Self::InvalidCharacter => {
                "The folder name contains a character Windows does not allow."
            }
            Self::ReservedName => {
                "The folder uses a name Windows keeps for a device, such as CON or NUL."
            }
            Self::TrailingDotOrSpace => {
                "A folder name ends with a dot or a space, which Windows drops."
            }
            Self::TooLong => {
                "The folder path is longer than 150 characters; some symbols count as two."
            }
            Self::DriveRoot => "Setup cannot install directly into a drive root.",
            Self::ShellMetacharacter(character) => shell_message(*character),
            Self::RootExists => {
                "The folder already exists, and Setup only installs into a new folder."
            }
            Self::ReparsePoint => {
                "The folder path passes through a link, junction or cloud placeholder."
            }
            Self::ProtectedFolder => {
                "Windows or Navo uses this folder, so Setup cannot install there."
            }
            Self::NeedsAdministrator => {
                "Program Files needs administrator rights, and this version of Setup cannot ask for them yet."
            }
            Self::DriveNotLocal => "Setup can install only on a fixed or removable local drive.",
            Self::SubstDrive => {
                "This drive is a mapped alias for another folder, which Setup cannot use."
            }
            Self::NotWritable => "Setup cannot write to this folder.",
            Self::NotEnoughSpace => "This drive does not have enough free space for Navo.",
            Self::ProbeFailed => "Setup could not check this folder.",
        }
    }

    /// One user-facing line saying how to choose a folder that works.
    pub fn fix(&self) -> &'static str {
        match self {
            Self::NotAbsolute => "Choose a folder on a local drive, or type its full path.",
            Self::DotComponent => "Choose the folder again without . or .. in the path.",
            Self::NetworkOrDevicePath | Self::DriveNotLocal => "Choose a folder on a local drive.",
            Self::AlternateDataStream => "Remove the extra colon and choose the folder again.",
            Self::InvalidCharacter => {
                "Choose a name without <, >, \", |, ?, * or control characters."
            }
            Self::ReservedName => "Choose a folder name that is not a reserved device name.",
            Self::TrailingDotOrSpace => "Remove the dot or space at the end of the name.",
            Self::TooLong => "Choose a shorter path.",
            Self::DriveRoot => "Choose a folder inside the drive, such as C:\\Navo.",
            Self::ShellMetacharacter(_) => {
                "Choose a folder name without %, ^, & or !, such as C:\\Navo."
            }
            Self::RootExists => "Choose a new folder, or let Browse add a Navo folder.",
            Self::ReparsePoint => "Choose a folder whose path has no links or synced placeholders.",
            Self::ProtectedFolder => "Choose another folder, such as C:\\Navo.",
            Self::NeedsAdministrator => {
                r"Choose a folder such as C:\Navo, or keep the default folder."
            }
            Self::SubstDrive => "Choose the folder on its real drive.",
            Self::NotWritable => "Choose a folder you can write to, or fix its permissions.",
            Self::NotEnoughSpace => "Free some space or choose another drive.",
            Self::ProbeFailed => "Choose another folder, or check that you can write to it.",
        }
    }
}

/// The line for one command-shell metacharacter IW6 refuses.
fn shell_message(character: char) -> &'static str {
    match character {
        '%' => "The folder name contains %, which Setup cannot pass safely to the uninstaller.",
        '^' => "The folder name contains ^, which Setup cannot pass safely to the uninstaller.",
        '&' => "The folder name contains &, which Setup cannot pass safely to the uninstaller.",
        '!' => "The folder name contains !, which Setup cannot pass safely to the uninstaller.",
        _ => "The folder name contains a character Setup cannot pass safely to the uninstaller.",
    }
}

/// Parses a Windows folder path, refusing every shape IW6 forbids.
pub fn parse_candidate(path: &OsStr) -> Result<Candidate, FolderProblem> {
    let text = path.to_str().ok_or(FolderProblem::InvalidCharacter)?;
    if text.encode_utf16().count() > MAX_UNITS {
        return Err(FolderProblem::TooLong);
    }
    if text.starts_with(r"\\") || text.starts_with("//") {
        return Err(FolderProblem::NetworkOrDevicePath);
    }
    if is_bare_device_name(text) {
        return Err(FolderProblem::ReservedName);
    }
    let rest = rest_after_drive(text)?;
    if rest.chars().all(|c| matches!(c, '\\' | '/')) {
        return Err(FolderProblem::DriveRoot);
    }
    check_components(rest)?;
    Ok(Candidate(PathBuf::from(text)))
}

/// Returns the text after `X:\` or `X:/`, or `NotAbsolute`.
fn rest_after_drive(text: &str) -> Result<&str, FolderProblem> {
    let bytes = text.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return Err(FolderProblem::NotAbsolute);
    }
    if !matches!(bytes[2], b'\\' | b'/') {
        return Err(FolderProblem::NotAbsolute);
    }
    Ok(&text[3..])
}

/// Whether the whole text names one device, such as `CON`, rather than a path.
fn is_bare_device_name(text: &str) -> bool {
    let mut names = text.split(['\\', '/']).filter(|part| !part.is_empty());
    let Some(name) = names.next() else {
        return false;
    };
    names.next().is_none() && is_reserved(name)
}

/// Checks each name between separators; empty names from doubled or trailing separators are skipped.
fn check_components(rest: &str) -> Result<(), FolderProblem> {
    for component in rest.split(['\\', '/']) {
        if component.is_empty() {
            continue;
        }
        if matches!(component, "." | "..") {
            return Err(FolderProblem::DotComponent);
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(FolderProblem::TrailingDotOrSpace);
        }
        if let Some(character) = component
            .chars()
            .find(|c| matches!(c, '%' | '^' | '&' | '!'))
        {
            return Err(FolderProblem::ShellMetacharacter(character));
        }
        if is_reserved(component) {
            return Err(FolderProblem::ReservedName);
        }
        if component.contains(':') {
            return Err(FolderProblem::AlternateDataStream);
        }
        if component.chars().any(is_invalid) {
            return Err(FolderProblem::InvalidCharacter);
        }
    }
    Ok(())
}

/// Whether one name is a Windows device name, with or without an extension.
fn is_reserved(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    RESERVED_NAMES
        .iter()
        .any(|name| stem.eq_ignore_ascii_case(name))
}

/// Whether Windows rejects this character in a file or folder name.
fn is_invalid(character: char) -> bool {
    character <= '\u{1f}' || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
}
