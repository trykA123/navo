//! The user command file: TOML into commands and line errors.

use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;
use toml::Spanned;

use crate::editor::Search;
use crate::terminal::split;

mod key;
mod shell;
mod template;

use self::key::normalize_key;
use self::shell::Shell;
pub(super) use self::shell::{is_shell_word, is_terminal, terminal_error};
use self::template::{first_placeholder, literal_text, template};

/// How the started program gets its console.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Console {
    /// A console window is shown.
    #[default]
    Show,
    /// The program runs without a window.
    Hide,
}

/// One command from `user-commands.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserCommand {
    pub name: String,
    pub program: String,
    pub args: Vec<Token>,
    pub start_in: Vec<Token>,
    pub console: Console,
    pub key: Option<String>,
}

/// One argument of a command, split into literal text and placeholders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub parts: Vec<Part>,
}

/// A piece of an argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Literal(String),
    Placeholder(Placeholder),
}

/// A path or name that a command line may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placeholder {
    File,
    Name,
    Stem,
    Ext,
    Folder,
    OtherFolder,
    OtherFile,
    Marked,
    MarkedList,
}

impl Placeholder {
    /// The name between braces, without the braces.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Name => "name",
            Self::Stem => "stem",
            Self::Ext => "ext",
            Self::Folder => "folder",
            Self::OtherFolder => "other-folder",
            Self::OtherFile => "other-file",
            Self::Marked => "marked",
            Self::MarkedList => "marked-list",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "file" => Self::File,
            "name" => Self::Name,
            "stem" => Self::Stem,
            "ext" => Self::Ext,
            "folder" => Self::Folder,
            "other-folder" => Self::OtherFolder,
            "other-file" => Self::OtherFile,
            "marked" => Self::Marked,
            "marked-list" => Self::MarkedList,
            _ => return None,
        })
    }
}

/// What is wrong in the file and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for LineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for LineError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    command: Vec<Spanned<Raw>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: Spanned<String>,
    program: Spanned<String>,
    #[serde(default)]
    args: Option<Spanned<String>>,
    #[serde(default, rename = "start-in")]
    start_in: Option<Spanned<String>>,
    #[serde(default)]
    console: Option<Console>,
    #[serde(default)]
    key: Option<Spanned<String>>,
}

/// Reads the command file. Every problem in the file is returned with its line.
pub fn parse(text: &str, search: &Search) -> Result<Vec<UserCommand>, Vec<LineError>> {
    parse_inner(text, Some(search))
}

#[cfg(test)]
pub(super) fn parse_without_identity(text: &str) -> Result<Vec<UserCommand>, Vec<LineError>> {
    parse_inner(text, None)
}

fn parse_inner(text: &str, search: Option<&Search>) -> Result<Vec<UserCommand>, Vec<LineError>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let File { command } = match toml::from_str(text) {
        Ok(file) => file,
        Err(error) => return Err(vec![syntax_error(text, &error)]),
    };
    let mut commands = Vec::new();
    let mut errors = Vec::new();
    let mut names = HashMap::new();
    for entry in command {
        let (command, mut found) = read_command(text, entry, &mut names, search);
        errors.append(&mut found);
        if let Some(command) = command {
            commands.push(command);
        }
    }
    errors.sort_by_key(|error| error.line);
    if errors.is_empty() {
        Ok(commands)
    } else {
        Err(errors)
    }
}

fn read_command(
    text: &str,
    entry: Spanned<Raw>,
    names: &mut HashMap<String, usize>,
    search: Option<&Search>,
) -> (Option<UserCommand>, Vec<LineError>) {
    let block_line = line_at(text, entry.span().start);
    let raw = entry.into_inner();
    let mut errors = Vec::new();
    let name = read_name(text, &raw.name, names, &mut errors);
    let program_line = line_at(text, raw.program.span().start);
    let program = read_program(text, &raw.program, &mut errors);
    let (args, words, args_line) = read_args(text, raw.args.as_ref(), block_line, &mut errors);
    let (start_in, start_in_line) =
        read_start_in(text, raw.start_in.as_ref(), block_line, &mut errors);
    let console = raw.console.unwrap_or_default();
    let key = read_key(text, raw.key.as_ref(), &mut errors);
    let Some(program) = program else {
        return (None, errors);
    };
    let shell = Shell {
        program: &program,
        words: &words,
        args: &args,
        start_in: &start_in,
        program_line,
        args_line,
        start_in_line,
        resolved_shell: search.is_some_and(|search| super::program::load_shell(&program, search)),
        default_start: raw.start_in.is_none(),
    };
    validate_program(shell, search, &mut errors);
    let command = UserCommand {
        name,
        program,
        args,
        start_in,
        console,
        key,
    };
    if errors.is_empty() {
        (Some(command), errors)
    } else {
        (None, errors)
    }
}

fn validate_program(shell: Shell<'_>, search: Option<&Search>, errors: &mut Vec<LineError>) {
    if let Some(search) = search
        && note_unknown_program(&shell, search, errors)
    {
        return;
    }
    if let Some(error) = shell.error() {
        errors.push(error);
        return;
    }
    if let Some(message) = terminal_error(shell.program, shell.args) {
        errors.push(LineError {
            line: shell.args_line,
            message: message.to_owned(),
        });
    }
}

fn note_unknown_program(shell: &Shell<'_>, search: &Search, errors: &mut Vec<LineError>) -> bool {
    if !has_placeholders(shell.args, shell.start_in)
        || !super::program::load_uncertain(shell.program, search)
    {
        return false;
    }
    errors.push(LineError {
        line: shell.program_line,
        message: super::program::UNKNOWN_PROGRAM.to_owned(),
    });
    true
}

fn has_placeholders(args: &[Token], start_in: &[Token]) -> bool {
    args.iter().chain(start_in).any(|token| {
        token
            .parts
            .iter()
            .any(|part| matches!(part, Part::Placeholder(_)))
    })
}

fn read_name(
    text: &str,
    name: &Spanned<String>,
    names: &mut HashMap<String, usize>,
    errors: &mut Vec<LineError>,
) -> String {
    let line = line_at(text, name.span().start);
    let name = name.get_ref().clone();
    if name.trim().is_empty() {
        errors.push(LineError {
            line,
            message: "`name` cannot be empty".to_owned(),
        });
    } else if let Some(first) = names.insert(name.clone(), line) {
        errors.push(LineError {
            line,
            message: format!("`{name}` is already defined at line {first}"),
        });
    }
    name
}

fn read_program(
    text: &str,
    program: &Spanned<String>,
    errors: &mut Vec<LineError>,
) -> Option<String> {
    let line = line_at(text, program.span().start);
    let token = match template(program.get_ref()) {
        Ok(token) => token,
        Err(message) => {
            errors.push(LineError { line, message });
            return None;
        }
    };
    if let Some(placeholder) = first_placeholder(&token) {
        errors.push(LineError {
            line,
            message: format!(
                "`program` cannot contain `{{{}}}`; it is the program to start",
                placeholder.name()
            ),
        });
        return None;
    }
    let program = literal_text(&token);
    if program.trim().is_empty() {
        errors.push(LineError {
            line,
            message: "`program` cannot be empty".to_owned(),
        });
        None
    } else {
        Some(program)
    }
}

fn read_args(
    text: &str,
    args: Option<&Spanned<String>>,
    block_line: usize,
    errors: &mut Vec<LineError>,
) -> (Vec<Token>, Vec<String>, usize) {
    let Some(args) = args else {
        return (Vec::new(), Vec::new(), block_line);
    };
    let line = line_at(text, args.span().start);
    let words = match split(args.get_ref()) {
        Ok(words) => words,
        Err(message) => {
            errors.push(LineError { line, message });
            return (Vec::new(), Vec::new(), line);
        }
    };
    let mut tokens = Vec::with_capacity(words.len());
    for word in &words {
        match template(word) {
            Ok(token) => tokens.push(token),
            Err(message) => errors.push(LineError { line, message }),
        }
    }
    (tokens, words, line)
}

fn read_start_in(
    text: &str,
    start_in: Option<&Spanned<String>>,
    block_line: usize,
    errors: &mut Vec<LineError>,
) -> (Vec<Token>, usize) {
    let Some(start_in) = start_in else {
        return (
            vec![Token {
                parts: vec![Part::Placeholder(Placeholder::Folder)],
            }],
            block_line,
        );
    };
    let line = line_at(text, start_in.span().start);
    let token = match template(start_in.get_ref()) {
        Ok(token) => token,
        Err(message) => {
            errors.push(LineError { line, message });
            return (Vec::new(), line);
        }
    };
    if token.parts.is_empty() {
        errors.push(LineError {
            line,
            message: "`start-in` cannot be empty".to_owned(),
        });
        return (Vec::new(), line);
    }
    if let Some(placeholder) = unsupported_start_in(&token) {
        errors.push(LineError {
            line,
            message: format!(
                "`{{{}}}` cannot be used in `start-in`; only `{{folder}}` and `{{other-folder}}` are allowed",
                placeholder.name()
            ),
        });
    }
    (vec![token], line)
}

fn unsupported_start_in(token: &Token) -> Option<Placeholder> {
    token.parts.iter().find_map(|part| match part {
        Part::Placeholder(placeholder)
            if !matches!(placeholder, Placeholder::Folder | Placeholder::OtherFolder) =>
        {
            Some(*placeholder)
        }
        Part::Placeholder(_) | Part::Literal(_) => None,
    })
}

fn read_key(
    text: &str,
    key: Option<&Spanned<String>>,
    errors: &mut Vec<LineError>,
) -> Option<String> {
    let key = key?;
    let line = line_at(text, key.span().start);
    match normalize_key(key.get_ref()) {
        Ok(key) => Some(key),
        Err(message) => {
            errors.push(LineError { line, message });
            None
        }
    }
}

fn syntax_error(text: &str, error: &toml::de::Error) -> LineError {
    let line = error.span().map_or(1, |span| line_at(text, span.start));
    LineError {
        line,
        message: error.message().to_owned(),
    }
}

fn line_at(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(1, |before| before.matches('\n').count() + 1)
}

#[cfg(test)]
mod tests;
