//! Expands a user command into the exact program, arguments and start folder.

#![allow(
    unsafe_code,
    reason = "GetLongPathNameW resolves 8.3 names before the shell check"
)]

use std::path::{Path, PathBuf};

use navo_core::Location;

use crate::editor::{Search, clean, resolve};

use super::model::{
    Console, Part, Placeholder, Token, UserCommand, is_shell_word, is_terminal, terminal_error,
};
use super::program;

/// The command line may not be longer than this, in UTF-16 units.
pub const MAX_COMMAND_LINE: usize = 32_000;

/// U3 replaces this marker in the arguments with the list file it creates.
///
/// The NUL makes `std::process::Command` reject the argument if the replacement
/// is forgotten, so a list path can never reach a program half-written.
pub const LIST_PATH_MARKER: &str = "\u{0}navo-user-command-list\u{0}";

/// The pane values a command may use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Invocation {
    /// The item under the cursor.
    pub file: Option<Location>,
    /// The marked items, in the order they are passed on.
    pub marked: Vec<Location>,
    /// The folder of the active pane.
    pub folder: Option<Location>,
    /// The folder of the copy-target pane.
    pub other_folder: Option<Location>,
    /// The item under the cursor in the copy-target pane.
    pub other_file: Option<Location>,
}

/// What a user command will start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub start_in: PathBuf,
    pub console: Console,
    /// The paths for the list file, when the command uses `{marked-list}`.
    pub list: Option<Vec<String>>,
}

/// Why a user command cannot run.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("there is no file under the cursor")]
    NoCursor,
    #[error("the cursor is on `..`")]
    DotDot,
    #[error("only local files can be used in a user command")]
    NonLocal,
    #[error("the current folder is not available")]
    NoFolder,
    #[error("the other pane has no folder")]
    NoOtherFolder,
    #[error("the other pane has no file under the cursor")]
    NoOtherFile,
    #[error("the start folder is missing")]
    MissingStart,
    #[error("the program was not found")]
    ProgramNotFound,
    #[error("the command line is over 32,000 UTF-16 units; use `{{marked-list}}` for many files")]
    TooLong,
    #[error("a command that starts a shell cannot use placeholders")]
    ShellPlaceholder,
    #[error("`{{marked}}` must be a whole argument")]
    MarkedNotWhole,
    #[error("`start-in` must be one value")]
    BadStartIn,
    #[error("the program name cannot be verified; placeholders are not allowed")]
    UnknownProgram,
    #[error("{0}")]
    Terminal(&'static str),
}

/// Expands the command for one invocation.
pub fn plan(
    command: &UserCommand,
    invocation: &Invocation,
    search: &Search,
) -> Result<Plan, Refusal> {
    validate_cursor(invocation)?;

    let program = clean(&command.program);
    let wt = is_terminal(&program);
    check_shell(is_shell_word(&program), command)?;

    let resolved = resolve(&program, search).ok_or(Refusal::ProgramNotFound)?;
    let placeholders = has_any_placeholder(command);
    if placeholders && program::resolved_uncertain(&resolved) {
        return Err(Refusal::UnknownProgram);
    }
    match is_shell_executable(&resolved, long_path) {
        Some(shell) => check_shell(shell, command)?,
        None if placeholders => return Err(Refusal::UnknownProgram),
        None => {}
    }
    if let Some(message) = terminal_error(&program, &command.args) {
        return Err(Refusal::Terminal(message));
    }
    let start_in = start_in(command, invocation)?;

    let mut list = None;
    let mut args = Vec::new();
    for token in &command.args {
        if has_marked(token) {
            if token.parts.len() != 1 {
                return Err(Refusal::MarkedNotWhole);
            }
            for path in marked_paths(invocation)? {
                args.push(path);
            }
        } else {
            let value = expand_token(token, invocation, &mut list)?;
            // Only literal text mixed with a placeholder can build a shell word.
            if !wt && mixes_literal(token) && is_shell_word(&value) {
                return Err(Refusal::ShellPlaceholder);
            }
            args.push(protect_dash(token, value));
        }
    }

    if command_line_units(&resolved, &args) > MAX_COMMAND_LINE {
        return Err(Refusal::TooLong);
    }
    Ok(Plan {
        program: resolved,
        args,
        start_in,
        console: command.console,
        list,
    })
}

fn validate_cursor(invocation: &Invocation) -> Result<(), Refusal> {
    file(invocation)?;
    Ok(())
}

fn check_shell(shell: bool, command: &UserCommand) -> Result<(), Refusal> {
    let shell = shell
        || command
            .args
            .iter()
            .any(|token| !has_placeholder(token) && is_shell_word(&literal_text(token)));
    if !shell {
        return Ok(());
    }
    if has_any_placeholder(command) {
        Err(Refusal::ShellPlaceholder)
    } else {
        Ok(())
    }
}

fn has_any_placeholder(command: &UserCommand) -> bool {
    command.start_in.is_empty()
        || command.start_in.iter().any(has_placeholder)
        || command.args.iter().any(has_placeholder)
}

fn is_shell_executable(
    path: &Path,
    long_path: impl FnOnce(&Path) -> Option<PathBuf>,
) -> Option<bool> {
    let path = long_path(path)?;
    Some(is_shell_word(&path.to_string_lossy()))
}

/// Whether the resolved program is a shell by its long name; the load-time twin of the run check.
pub(super) fn resolved_is_shell(path: &Path) -> bool {
    is_shell_executable(path, long_path) == Some(true)
}

#[cfg(windows)]
fn long_path(path: &Path) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetLongPathNameW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: wide is NUL-terminated; a null output with zero length asks for the needed size.
    let needed = unsafe { GetLongPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return None;
    }
    let mut buffer = vec![0u16; needed as usize];
    // SAFETY: buffer has needed writable units and wide stays NUL-terminated.
    let length = unsafe { GetLongPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), needed) };
    if length == 0 || length > needed {
        return None;
    }
    buffer.truncate(length as usize);
    Some(PathBuf::from(OsString::from_wide(&buffer)))
}

#[cfg(not(windows))]
fn long_path(path: &Path) -> Option<PathBuf> {
    Some(path.to_owned())
}

fn has_marked(token: &Token) -> bool {
    token
        .parts
        .iter()
        .any(|part| matches!(part, Part::Placeholder(Placeholder::Marked)))
}

fn has_placeholder(token: &Token) -> bool {
    token
        .parts
        .iter()
        .any(|part| matches!(part, Part::Placeholder(_)))
}

fn mixes_literal(token: &Token) -> bool {
    has_placeholder(token)
        && token
            .parts
            .iter()
            .any(|part| matches!(part, Part::Literal(_)))
}

/// A token that starts with a name part may not turn into an option.
fn protect_dash(token: &Token, value: String) -> String {
    let starts_with_name = matches!(
        token.parts.first(),
        Some(Part::Placeholder(
            Placeholder::Name | Placeholder::Stem | Placeholder::Ext
        ))
    );
    if !starts_with_name || !value.starts_with('-') {
        return value;
    }
    let dot = if cfg!(windows) { ".\\" } else { "./" };
    format!("{dot}{value}")
}

fn literal_text(token: &Token) -> String {
    token
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Literal(text) => Some(text.as_str()),
            Part::Placeholder(_) => None,
        })
        .collect()
}

fn expand_token(
    token: &Token,
    invocation: &Invocation,
    list: &mut Option<Vec<String>>,
) -> Result<String, Refusal> {
    let mut value = String::new();
    for part in &token.parts {
        match part {
            Part::Literal(text) => value.push_str(text),
            Part::Placeholder(placeholder) => {
                let text = placeholder_value(*placeholder, invocation, list)?;
                value.push_str(&text);
            }
        }
    }
    Ok(value)
}

fn placeholder_value(
    placeholder: Placeholder,
    invocation: &Invocation,
    list: &mut Option<Vec<String>>,
) -> Result<String, Refusal> {
    match placeholder {
        Placeholder::File => Ok(file(invocation)?.as_str().to_owned()),
        Placeholder::Name => name(invocation),
        Placeholder::Stem => Ok(stem_and_ext(invocation)?.0),
        Placeholder::Ext => Ok(stem_and_ext(invocation)?.1),
        Placeholder::Folder => Ok(folder(invocation)?.as_str().to_owned()),
        Placeholder::OtherFolder => Ok(other_folder(invocation)?.as_str().to_owned()),
        Placeholder::OtherFile => Ok(other_file(invocation)?.as_str().to_owned()),
        Placeholder::Marked => Err(Refusal::MarkedNotWhole),
        Placeholder::MarkedList => {
            if list.is_none() {
                *list = Some(marked_paths(invocation)?);
            }
            Ok(LIST_PATH_MARKER.to_owned())
        }
    }
}

fn name(invocation: &Invocation) -> Result<String, Refusal> {
    let file = file(invocation)?;
    file.name()
        .map(|name| name.display().into_owned())
        .ok_or(Refusal::NoCursor)
}

fn stem_and_ext(invocation: &Invocation) -> Result<(String, String), Refusal> {
    let name = name(invocation)?;
    let path = Path::new(&name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&name)
        .to_owned();
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_owned();
    Ok((stem, ext))
}

fn file(invocation: &Invocation) -> Result<&Location, Refusal> {
    let file = invocation.file.as_ref().ok_or(Refusal::NoCursor)?;
    local(file)?;
    check_dot_dot(file)?;
    Ok(file)
}

fn folder(invocation: &Invocation) -> Result<&Location, Refusal> {
    let folder = invocation.folder.as_ref().ok_or(Refusal::NoFolder)?;
    local(folder)?;
    Ok(folder)
}

fn other_folder(invocation: &Invocation) -> Result<&Location, Refusal> {
    let folder = invocation
        .other_folder
        .as_ref()
        .ok_or(Refusal::NoOtherFolder)?;
    local(folder)?;
    Ok(folder)
}

fn other_file(invocation: &Invocation) -> Result<&Location, Refusal> {
    let file = invocation.other_file.as_ref().ok_or(Refusal::NoOtherFile)?;
    local(file)?;
    check_dot_dot(file)?;
    Ok(file)
}

fn local(location: &Location) -> Result<(), Refusal> {
    if location.is_local() {
        Ok(())
    } else {
        Err(Refusal::NonLocal)
    }
}

fn check_dot_dot(location: &Location) -> Result<(), Refusal> {
    let last = location
        .as_str()
        .trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next();
    if last == Some("..") {
        return Err(Refusal::DotDot);
    }
    match location.name() {
        Some(_) => Ok(()),
        None => Err(Refusal::NoCursor),
    }
}

fn marked_paths(invocation: &Invocation) -> Result<Vec<String>, Refusal> {
    if invocation.marked.is_empty() {
        return Ok(vec![file(invocation)?.as_str().to_owned()]);
    }
    let mut paths = Vec::with_capacity(invocation.marked.len());
    for location in &invocation.marked {
        local(location)?;
        check_dot_dot(location)?;
        paths.push(location.as_str().to_owned());
    }
    Ok(paths)
}

fn start_in(command: &UserCommand, invocation: &Invocation) -> Result<PathBuf, Refusal> {
    let value = if command.start_in.is_empty() {
        folder(invocation)?.as_str().to_owned()
    } else if command.start_in.len() != 1 {
        return Err(Refusal::BadStartIn);
    } else {
        let mut value = String::new();
        for part in &command.start_in[0].parts {
            match part {
                Part::Literal(text) => value.push_str(text),
                Part::Placeholder(Placeholder::Folder) => {
                    value.push_str(folder(invocation)?.as_str());
                }
                Part::Placeholder(Placeholder::OtherFolder) => {
                    value.push_str(other_folder(invocation)?.as_str());
                }
                Part::Placeholder(_) => return Err(Refusal::BadStartIn),
            }
        }
        value
    };
    let path = PathBuf::from(value);
    if !path.is_dir() {
        return Err(Refusal::MissingStart);
    }
    Ok(path)
}

/// The Windows command line std builds, in UTF-16 units: the program, one
/// space before each argument, and each value quoted and escaped as needed.
pub(super) fn command_line_units(program: &Path, args: &[String]) -> usize {
    let mut units = serialized_units(&program.to_string_lossy());
    for arg in args {
        units = units
            .saturating_add(1)
            .saturating_add(serialized_units(arg));
    }
    units
}

fn serialized_units(value: &str) -> usize {
    if !value.is_empty() && !value.chars().any(|ch| ch.is_whitespace() || ch == '"') {
        return value.encode_utf16().count();
    }
    let mut units = 1usize;
    let mut backslashes = 0usize;
    for ch in value.chars() {
        match ch {
            '\\' => {
                backslashes = backslashes.saturating_add(1);
            }
            '"' => {
                units = units
                    .saturating_add(backslashes.saturating_mul(2))
                    .saturating_add(2);
                backslashes = 0;
            }
            _ => {
                units = units
                    .saturating_add(backslashes)
                    .saturating_add(ch.len_utf16());
                backslashes = 0;
            }
        }
    }
    units
        .saturating_add(backslashes.saturating_mul(2))
        .saturating_add(1)
}

#[cfg(test)]
mod tests;
