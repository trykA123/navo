//! Argument parser for Sidewise: flags, side specs and exit codes.
//! It is pure: the caller supplies the test for "this path exists".
//! The app and the console launcher both call `parse`.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

pub use crate::side_arg::side;

/// Most arguments one call may carry (NC23).
pub const MAX_ARGS: usize = 256;

/// Most bytes all arguments together may carry (NC23).
pub const MAX_BYTES: usize = 64 * 1024;

/// One parsed command line: what to open and the flags around it.
#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub action: Action,
    pub wait: bool,
    pub new_window: bool,
    pub read_only: bool,
    pub profile: Option<PathBuf>,
    pub left_title: Option<String>,
    pub right_title: Option<String>,
}

/// What the command line asks the app to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Start { left: Option<SideSpec> },
    Compare { left: SideSpec, right: SideSpec },
    SelectLeft { path: PathBuf },
    CompareWith { right: PathBuf },
    Version,
    Help,
}

/// One side the user named: a path, or a repository at a revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideSpec {
    Path(PathBuf),
    GitRev {
        repo: PathBuf,
        rev: String,
    },
    /// A revision of a remote https repository: canonical address and revision (`HEAD` when none was typed).
    GitUrl {
        url: String,
        rev: String,
    },
}

/// Why a command line was refused, before any file or program is touched.
#[derive(Debug, PartialEq, Eq)]
pub enum ArgError {
    UnknownFlag(String),
    MissingValue(&'static str),
    NotUnicode,
    BadRev(String),
    BadUrl(String),
    ThreeWay,
    PickWithSides,
    TooManyArgs(usize),
    TooLong(usize),
}

/// Process exit codes of NC10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Exit {
    /// Closed, or saved with no conflict left.
    Ok = 0,
    /// Closed with unsaved changes discarded.
    Discarded = 1,
    /// The command line or the merge failed.
    Error = 2,
}

impl Invocation {
    /// Builds the invocation for `--version` or `--help`, with every flag at its default.
    fn bare(action: Action) -> Self {
        Self {
            action,
            wait: false,
            new_window: false,
            read_only: false,
            profile: None,
            left_title: None,
            right_title: None,
        }
    }
}

impl fmt::Display for ArgError {
    /// Writes one plain sentence that names the refused part; user text is printed escaped,
    /// so a control character in an argument never reaches the terminal.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFlag(flag) => write!(f, "unknown option {flag:?}."),
            Self::MissingValue(flag) => write!(f, "option `{flag}` needs a value."),
            Self::NotUnicode => write!(f, "an argument is not valid Unicode."),
            Self::BadRev(rev) => write!(f, "the revision {rev:?} is not valid."),
            Self::BadUrl(text) => write!(
                f,
                "{text:?} is not a supported address: only https:// addresses without a user name are supported."
            ),
            Self::ThreeWay => write!(f, "three-way merge arrives in a later release."),
            Self::PickWithSides => write!(
                f,
                "options `--select-left` and `--compare-with` take one path and no other side."
            ),
            Self::TooManyArgs(count) => write!(f, "too many arguments: {count}."),
            Self::TooLong(bytes) => write!(f, "the arguments are too long: {bytes} bytes."),
        }
    }
}

/// Parses `args` without the program name into an invocation.
/// Relative paths join `cwd`; `exists` tests a path without touching the disk.
/// Fails with the first problem: the size limits, then a flag or side error.
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> Result<Invocation, ArgError> {
    precheck(args)?;
    let scan = scan(args);
    if scan.wants_version {
        return Ok(Invocation::bare(Action::Version));
    }
    if scan.wants_help {
        return Ok(Invocation::bare(Action::Help));
    }
    if let Some(error) = scan.error {
        return Err(error);
    }
    let action = scan.action(cwd, exists)?;
    Ok(Invocation {
        action,
        wait: scan.wait,
        new_window: scan.new_window,
        read_only: scan.read_only,
        profile: scan.profile,
        left_title: scan.left_title,
        right_title: scan.right_title,
    })
}

/// Refuses a call over the argument count, the byte total or invalid Unicode.
fn precheck(args: &[OsString]) -> Result<(), ArgError> {
    if args.len() > MAX_ARGS {
        return Err(ArgError::TooManyArgs(args.len()));
    }
    let total: usize = args.iter().map(|arg| arg.as_os_str().len()).sum();
    if total > MAX_BYTES {
        return Err(ArgError::TooLong(total));
    }
    if args.iter().any(|arg| arg.to_str().is_none()) {
        return Err(ArgError::NotUnicode);
    }
    Ok(())
}

/// What one left-to-right pass over the arguments collects.
#[derive(Default)]
struct Scan {
    wait: bool,
    new_window: bool,
    read_only: bool,
    profile: Option<PathBuf>,
    left_title: Option<String>,
    right_title: Option<String>,
    select_left: Option<String>,
    compare_with: Option<String>,
    positionals: Vec<String>,
    wants_version: bool,
    wants_help: bool,
    error: Option<ArgError>,
}

impl Scan {
    /// Keeps the first error seen; a later error does not replace it.
    fn fail(&mut self, error: ArgError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    /// Stores a value flag's last value.
    fn set_value(&mut self, flag: &'static str, value: String) {
        if value.is_empty() && matches!(flag, "--select-left" | "--compare-with") {
            self.fail(ArgError::MissingValue(flag));
            return;
        }
        match flag {
            "--profile" => self.profile = Some(PathBuf::from(value)),
            "--left-title" => self.left_title = Some(value),
            "--right-title" => self.right_title = Some(value),
            "--select-left" => self.select_left = Some(value),
            "--compare-with" => self.compare_with = Some(value),
            _ => {}
        }
    }

    /// Sets the boolean flag named `flag`.
    fn set_bool(&mut self, flag: &str) {
        match flag {
            "--wait" => self.wait = true,
            "--new-window" => self.new_window = true,
            "--read-only" => self.read_only = true,
            "--version" => self.wants_version = true,
            "--help" => self.wants_help = true,
            _ => {}
        }
    }

    /// Handles a long flag written as `--flag=value`.
    fn inline_value(&mut self, whole: &str, name: &str, value: &str) {
        if let Some(flag) = value_flag(name) {
            if value.is_empty() {
                self.fail(ArgError::MissingValue(flag));
            } else {
                self.set_value(flag, value.to_owned());
            }
        } else {
            self.fail(ArgError::UnknownFlag(whole.to_owned()));
        }
    }

    /// Takes the next item as a value or records the error; returns the next index.
    fn separate(&mut self, args: &[OsString], index: usize, name: &str) -> usize {
        if let Some(flag) = value_flag(name) {
            let Some(next) = args.get(index + 1) else {
                self.fail(ArgError::MissingValue(flag));
                return index + 1;
            };
            match next.to_str() {
                Some(value) => self.set_value(flag, value.to_owned()),
                None => self.fail(ArgError::NotUnicode),
            }
            index + 2
        } else if bool_flag(name) {
            self.set_bool(name);
            index + 1
        } else {
            self.fail(ArgError::UnknownFlag(name.to_owned()));
            index + 1
        }
    }

    /// Builds the action from the positionals, parsing each side in order.
    fn action(&self, cwd: &Path, exists: &dyn Fn(&Path) -> bool) -> Result<Action, ArgError> {
        let pick = match (&self.select_left, &self.compare_with) {
            (Some(_), Some(_)) => return Err(ArgError::PickWithSides),
            (Some(left), None) => Some(Action::SelectLeft {
                path: cwd.join(left),
            }),
            (None, Some(right)) => Some(Action::CompareWith {
                right: cwd.join(right),
            }),
            (None, None) => None,
        };
        if let Some(action) = pick {
            if self.positionals.is_empty() {
                return Ok(action);
            }
            return Err(ArgError::PickWithSides);
        }
        match self.positionals.as_slice() {
            [] => Ok(Action::Start { left: None }),
            [one] => Ok(Action::Start {
                left: Some(side(one, cwd, exists)?),
            }),
            [left, right] => Ok(Action::Compare {
                left: side(left, cwd, exists)?,
                right: side(right, cwd, exists)?,
            }),
            [_, _, _] | [_, _, _, _] => Err(ArgError::ThreeWay),
            many => Err(ArgError::TooManyArgs(many.len())),
        }
    }
}

/// Splits `--flag=value` into its parts; other items are not long flags.
fn split_flag(text: &str) -> Option<(&str, Option<&str>)> {
    if !text.starts_with("--") {
        return None;
    }
    match text.split_once('=') {
        Some((name, value)) => Some((name, Some(value))),
        None => Some((text, None)),
    }
}

/// Names of the flags that take a value.
fn value_flag(name: &str) -> Option<&'static str> {
    match name {
        "--profile" => Some("--profile"),
        "--left-title" => Some("--left-title"),
        "--right-title" => Some("--right-title"),
        "--select-left" => Some("--select-left"),
        "--compare-with" => Some("--compare-with"),
        _ => None,
    }
}

/// Reports whether `name` is a flag that takes no value.
fn bool_flag(name: &str) -> bool {
    matches!(
        name,
        "--wait" | "--new-window" | "--read-only" | "--version" | "--help"
    )
}

/// Reads all arguments left to right and records flags, positionals and the first error.
fn scan(args: &[OsString]) -> Scan {
    let mut scan = Scan::default();
    let mut index = 0;
    let mut flags_ended = false;
    while let Some(raw) = args.get(index) {
        let Some(text) = raw.to_str() else {
            scan.fail(ArgError::NotUnicode);
            index += 1;
            continue;
        };
        if flags_ended {
            scan.positionals.push(text.to_owned());
            index += 1;
            continue;
        }
        if text == "--" {
            flags_ended = true;
            index += 1;
            continue;
        }
        if text == "-" {
            scan.positionals.push(text.to_owned());
            index += 1;
            continue;
        }
        if text == "-o" {
            scan.fail(ArgError::ThreeWay);
            index += 1;
            continue;
        }
        if let Some((name, value)) = split_flag(text) {
            match value {
                Some(value) => {
                    scan.inline_value(text, name, value);
                    index += 1;
                }
                None => index = scan.separate(args, index, name),
            }
            continue;
        }
        if text.starts_with('-') {
            scan.fail(ArgError::UnknownFlag(text.to_owned()));
        } else {
            scan.positionals.push(text.to_owned());
        }
        index += 1;
    }
    scan
}
