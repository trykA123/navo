//! Why a user command did not start; the run hook shows each case as a localized message
//! that ends with the next step.

use std::io;

use super::expand::Refusal;

/// Why a user command did not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserCommandError {
    /// No command in `user-commands.toml` has the requested name.
    NoCommand,
    /// The command cannot run for this cursor, pane or program.
    Refused(Refusal),
    /// The worker thread that runs the command did not start.
    Spawn(io::ErrorKind),
    /// The program or its list file did not start.
    Start(StartError),
}

/// Why the program or its list file did not start. OS failures keep only their `io::ErrorKind`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartError {
    /// Neither the local app data folder nor the home folder is known.
    NoListRoot,
    /// The list folder or its parent could not be created or inspected.
    ListFolder(io::ErrorKind),
    /// The list folder, its parent or a pid sidecar's folder is not a private, real folder.
    ListFolderNotPrivate,
    /// A marked path contains a line break, which would split it in the list file.
    ListLineBreak,
    /// The list file could not be created, written or kept.
    ListFile(io::ErrorKind),
    /// The pid sidecar next to the list file could not be created, written or published.
    Sidecar(io::ErrorKind),
    /// The marked-list placeholder has no list file or could not be filled in.
    ListMarker,
    /// The command line is over the Windows limit of 32,000 UTF-16 units.
    TooLong,
    /// The thread that waits for the program did not start.
    Waiter(io::ErrorKind),
    /// The operating system did not start the program.
    Program(io::ErrorKind),
}

impl From<io::Error> for UserCommandError {
    /// Keeps only the kind of a thread start failure.
    fn from(error: io::Error) -> Self {
        Self::Spawn(error.kind())
    }
}

impl UserCommandError {
    /// The text in the current locale for this failure; `name` is the command the user ran.
    pub fn message(&self, name: &str) -> String {
        match self {
            Self::NoCommand => navo_l10n::t!("user-command-missing", name = name.to_owned()),
            Self::Refused(refusal) => refusal_message(refusal),
            Self::Spawn(kind) => {
                navo_l10n::t!("user-command-spawn-error", error = kind.to_string())
            }
            Self::Start(error) => start_message(error),
        }
    }
}

/// The text for one refusal. `Terminal` has a single cause today, so it has one id.
fn refusal_message(refusal: &Refusal) -> String {
    match refusal {
        Refusal::NoCursor => navo_l10n::tr("user-command-no-cursor"),
        Refusal::DotDot => navo_l10n::tr("user-command-dot-dot"),
        Refusal::NonLocal => navo_l10n::tr("user-command-not-local"),
        Refusal::NoFolder => navo_l10n::tr("user-command-no-folder"),
        Refusal::NoOtherFolder => navo_l10n::tr("user-command-no-other-folder"),
        Refusal::NoOtherFile => navo_l10n::tr("user-command-no-other-file"),
        Refusal::MissingStart => navo_l10n::tr("user-command-missing-start"),
        Refusal::ProgramNotFound => navo_l10n::tr("user-command-program-missing"),
        Refusal::TooLong => navo_l10n::tr("user-command-too-long"),
        Refusal::ShellPlaceholder => navo_l10n::tr("user-command-shell-placeholder"),
        Refusal::MarkedNotWhole => navo_l10n::tr("user-command-marked-not-whole"),
        Refusal::BadStartIn => navo_l10n::tr("user-command-bad-start-in"),
        Refusal::UnknownProgram => navo_l10n::tr("user-command-unknown-program"),
        Refusal::Terminal(_) => navo_l10n::tr("user-command-terminal-placeholder"),
    }
}

/// The text for one start failure; OS failures show their `io::ErrorKind` in brackets.
fn start_message(error: &StartError) -> String {
    let kind = |kind: &io::ErrorKind| kind.to_string();
    match error {
        StartError::NoListRoot => navo_l10n::tr("user-command-list-root"),
        StartError::ListFolder(error) => {
            navo_l10n::t!("user-command-list-folder-error", error = kind(error))
        }
        StartError::ListFolderNotPrivate => navo_l10n::tr("user-command-list-folder-unsafe"),
        StartError::ListLineBreak => navo_l10n::tr("user-command-list-line-break"),
        StartError::ListFile(error) => {
            navo_l10n::t!("user-command-list-file-error", error = kind(error))
        }
        StartError::Sidecar(error) => {
            navo_l10n::t!("user-command-pid-file-error", error = kind(error))
        }
        StartError::ListMarker => navo_l10n::tr("user-command-list-marker"),
        StartError::TooLong => navo_l10n::tr("user-command-too-long"),
        StartError::Waiter(error) => {
            navo_l10n::t!("user-command-spawn-error", error = kind(error))
        }
        StartError::Program(error) => {
            navo_l10n::t!("user-command-start-error", error = kind(error))
        }
    }
}
