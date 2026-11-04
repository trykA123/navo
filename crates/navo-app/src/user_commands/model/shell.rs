//! The shell rule: a command that starts a shell may not use placeholders.

use crate::terminal::is_shell_program;

use super::template::first_placeholder;
use super::{LineError, Part, Placeholder, Token};

pub(super) struct Shell<'a> {
    pub(super) program: &'a str,
    pub(super) words: &'a [String],
    pub(super) args: &'a [Token],
    pub(super) start_in: &'a [Token],
    pub(super) program_line: usize,
    pub(super) args_line: usize,
    pub(super) start_in_line: usize,
    /// The program resolves to a shell, such as `code.cmd`.
    pub(super) resolved_shell: bool,
    /// `start-in` is absent, so the default `{folder}` stands in.
    pub(super) default_start: bool,
}

impl Shell<'_> {
    pub(super) fn error(&self) -> Option<LineError> {
        let shell = self.resolved_shell
            || is_shell_word(self.program)
            || self.words.iter().any(|word| is_shell_word(word));
        if !shell {
            return None;
        }
        if let Some(placeholder) = self.args.iter().find_map(first_placeholder) {
            return Some(error(self.args_line, placeholder));
        }
        let placeholder = self.start_in.iter().find_map(first_placeholder)?;
        if self.default_start {
            return Some(LineError {
                line: self.start_in_line,
                message: "a command that starts a shell needs a literal `start-in`; the default start folder counts as `{folder}`".to_owned(),
            });
        }
        Some(error(self.start_in_line, placeholder))
    }
}

/// Normalizes a program word: surrounding quotes and spaces, trailing dots and
/// spaces, letter case, and a trailing `.exe`.
pub(super) fn clean_word(word: &str) -> String {
    let word = word
        .trim()
        .trim_matches('"')
        .trim()
        .trim_end_matches(['.', ' '])
        .to_ascii_lowercase();
    word.strip_suffix(".exe")
        .unwrap_or(word.as_str())
        .to_owned()
}

pub(crate) fn is_shell_word(word: &str) -> bool {
    let word = clean_word(word);
    is_shell_program(&word) || word.ends_with(".ps1")
}

/// Whether the program is Windows Terminal, by its cleaned file name.
pub(crate) fn is_terminal(program: &str) -> bool {
    let program = clean_word(program);
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program.as_str());
    name == "wt"
}

/// Why a Windows Terminal command cannot use a placeholder.
pub(crate) fn terminal_error(program: &str, args: &[Token]) -> Option<&'static str> {
    if !is_terminal(program) {
        return None;
    }
    if args.iter().any(has_placeholder) {
        return Some(
            "a Windows Terminal command cannot use placeholders; put the folder in `start-in` and write `wt -d .`",
        );
    }
    None
}

fn has_placeholder(token: &Token) -> bool {
    token
        .parts
        .iter()
        .any(|part| matches!(part, Part::Placeholder(_)))
}

fn error(line: usize, placeholder: Placeholder) -> LineError {
    LineError {
        line,
        message: format!(
            "a command that starts a shell cannot contain `{{{}}}`",
            placeholder.name()
        ),
    }
}
