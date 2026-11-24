//! Reads and checks each field of one command block: name, program, args, start-in and key.

use std::collections::HashMap;

use toml::Spanned;

use crate::terminal::split;

use super::key::normalize_key;
use super::template::{first_placeholder, literal_text, template};
use super::{LineError, Part, Placeholder, Token, line_at};

pub(super) fn read_name(
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

pub(super) fn read_program(
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

pub(super) fn read_args(
    text: &str,
    args: Option<&Spanned<String>>,
    block_line: usize,
    errors: &mut Vec<LineError>,
) -> (Vec<Token>, Vec<String>, usize) {
    let Some(args) = args else {
        return (Vec::new(), Vec::new(), block_line);
    };
    let line = line_at(text, args.span().start);
    let words = match split(args.get_ref()).map_err(|error| error.message()) {
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

pub(super) fn read_start_in(
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

pub(super) fn read_key(
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
