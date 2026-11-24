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
    /// The program or its list file did not start; the text is still English until AQ4b-2.
    Start(String),
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
            Self::Start(error) => navo_l10n::t!("user-command-start-error", error = error.clone()),
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
