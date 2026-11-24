//! Why a line of `user-commands.toml` is wrong. Each cause maps to a localized message that
//! names the line and ends with the next step.

use std::fmt;

use navo_l10n::t;

use super::Placeholder;

/// What is wrong in the file and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineError {
    /// The 1-based line in `user-commands.toml`.
    pub line: usize,
    pub error: ParseError,
}

/// What the user must fix on one line, grouped by cause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The file is not valid TOML or has unknown or missing fields; `detail` is the TOML reader's text.
    Syntax { detail: String },
    /// A field that needs a value is empty.
    Empty(Field),
    /// Another command above already uses this name.
    DuplicateName { name: String, first_line: usize },
    /// `program` contains a placeholder.
    ProgramPlaceholder(Placeholder),
    /// `args` leaves a quotation mark open.
    UnclosedQuote,
    /// A brace or placeholder in a value is wrong.
    Template(TemplateError),
    /// `start-in` uses a placeholder other than `{folder}` or `{other-folder}`.
    StartInPlaceholder(Placeholder),
    /// The `key` is not a shortcut the app can use.
    Key(KeyError),
    /// A command that starts a shell uses a placeholder.
    ShellPlaceholder(Placeholder),
    /// A command that starts a shell has no `start-in`, so the default `{folder}` stands in.
    ShellDefaultStart,
    /// A Windows Terminal command uses a placeholder in `args`.
    TerminalPlaceholder,
    /// The program is a link or alias, so a placeholder cannot be checked against it.
    UnknownProgram,
}

/// A field of a command block that cannot be empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Program,
    StartIn,
}

/// Why a value with braces cannot be split into text and placeholders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    /// A `}` with no `{` before it.
    LoneBrace,
    /// A `{` with no `}` after it.
    UnclosedBrace,
    /// The name between braces is not a known placeholder.
    UnknownPlaceholder { name: String },
    /// `{marked}` shares its argument with other text.
    MarkedNotWhole,
}

/// Why a `key` value is not a usable shortcut; `key` is the text as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyError {
    NotOneKey,
    NotAKey { key: String },
    ModifierAlone { key: String },
    Bare { key: String },
    Reserved { key: String },
}

impl Field {
    /// The field's name as written in the file.
    const fn name(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Program => "program",
            Self::StartIn => "start-in",
        }
    }
}

impl LineError {
    /// The text in the current locale; it names the line and ends with the next step.
    pub fn message(&self) -> String {
        let line = self.line.to_string();
        let placeholder = |placeholder: &Placeholder| placeholder.name().to_owned();
        match &self.error {
            ParseError::Syntax { detail } => t!(
                "user-command-file-syntax",
                line = line,
                detail = detail.clone()
            ),
            ParseError::Empty(field) => {
                t!("user-command-file-empty", line = line, field = field.name())
            }
            ParseError::DuplicateName { name, first_line } => t!(
                "user-command-file-duplicate-name",
                line = line,
                name = name.clone(),
                first = first_line.to_string()
            ),
            ParseError::ProgramPlaceholder(found) => t!(
                "user-command-file-program-placeholder",
                line = line,
                placeholder = placeholder(found)
            ),
            ParseError::UnclosedQuote => t!("user-command-file-unclosed-quote", line = line),
            ParseError::Template(error) => template_message(error, line),
            ParseError::StartInPlaceholder(found) => t!(
                "user-command-file-start-in-placeholder",
                line = line,
                placeholder = placeholder(found)
            ),
            ParseError::Key(error) => key_message(error, line),
            ParseError::ShellPlaceholder(found) => t!(
                "user-command-file-shell-placeholder",
                line = line,
                placeholder = placeholder(found)
            ),
            ParseError::ShellDefaultStart => {
                t!("user-command-file-shell-default-start", line = line)
            }
            ParseError::TerminalPlaceholder => {
                t!("user-command-file-terminal-placeholder", line = line)
            }
            ParseError::UnknownProgram => t!("user-command-file-unknown-program", line = line),
        }
    }
}

/// The text for a brace or placeholder error on `line`.
fn template_message(error: &TemplateError, line: String) -> String {
    match error {
        TemplateError::LoneBrace => t!("user-command-file-lone-brace", line = line),
        TemplateError::UnclosedBrace => t!("user-command-file-unclosed-brace", line = line),
        TemplateError::UnknownPlaceholder { name } => t!(
            "user-command-file-unknown-placeholder",
            line = line,
            name = name.clone()
        ),
        TemplateError::MarkedNotWhole => t!("user-command-file-marked-not-whole", line = line),
    }
}

/// The text for a shortcut error on `line`.
fn key_message(error: &KeyError, line: String) -> String {
    match error {
        KeyError::NotOneKey => t!("user-command-file-key-not-one", line = line),
        KeyError::NotAKey { key } => t!(
            "user-command-file-not-a-key",
            line = line,
            key = key.clone()
        ),
        KeyError::ModifierAlone { key } => t!(
            "user-command-file-key-modifier",
            line = line,
            key = key.clone()
        ),
        KeyError::Bare { key } => t!("user-command-file-key-bare", line = line, key = key.clone()),
        KeyError::Reserved { key } => t!(
            "user-command-file-key-reserved",
            line = line,
            key = key.clone()
        ),
    }
}

impl fmt::Display for LineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for LineError {}
