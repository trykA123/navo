use std::collections::{HashMap, HashSet};
use std::fmt;

use gpui_kit::{KeyBindingContextPredicate, Keystroke};
use serde::Deserialize;
use toml::Spanned;

use super::Bind;
use crate::commands::{CommandSpec, GLOBAL, by_name, is_within};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum KeymapError {
    Syntax(String),
    UnknownCommand {
        command: String,
    },
    BadKeystroke {
        command: String,
        key: String,
    },
    ContextNotAllowed {
        command: String,
        context: String,
    },
    DuplicateKey {
        key: String,
        context: String,
    },
    DuplicateEntry {
        command: String,
        context: String,
    },
    UppercaseKey {
        command: String,
        key: String,
    },
    KeyConflict {
        key: String,
        context: String,
        first: String,
        second: String,
    },
}

impl fmt::Display for KeymapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(message) => write!(formatter, "the keymap is not valid: {message}"),
            Self::UnknownCommand { command } => write!(formatter, "unknown command `{command}`"),
            Self::BadKeystroke { command, key } => {
                write!(formatter, "`{key}` is not a key (command `{command}`)")
            }
            Self::ContextNotAllowed { command, context } => {
                write!(formatter, "`{command}` cannot be bound in `{context}`")
            }
            Self::DuplicateKey { key, context } => {
                write!(formatter, "`{key}` is bound twice in `{context}`")
            }
            Self::DuplicateEntry { command, context } => {
                write!(formatter, "`{command}` has two entries for `{context}`")
            }
            Self::UppercaseKey { command, key } => write!(
                formatter,
                "`{key}` must be lower case, like `{}` (command `{command}`)",
                key.to_lowercase()
            ),
            Self::KeyConflict {
                key,
                context,
                first,
                second,
            } => write!(
                formatter,
                "`{key}` is given to both `{first}` and `{second}` in `{context}`"
            ),
        }
    }
}

impl std::error::Error for KeymapError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    bind: Vec<Spanned<Raw>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    command: String,
    keys: Vec<String>,
    context: Option<String>,
}

pub(super) struct Entry {
    pub(super) bind: Bind,
    pub(super) line: usize,
}

pub(super) type Located = (Option<usize>, KeymapError);

pub(super) fn parse(text: &str) -> Result<Vec<Bind>, KeymapError> {
    let mut seen = Seen::default();
    entries(text)
        .map_err(|(_, error)| error)?
        .into_iter()
        .map(|entry| seen.admit(entry.bind, false))
        .collect()
}

pub(super) fn entries(text: &str) -> Result<Vec<Entry>, Located> {
    let file: File = toml::from_str(text).map_err(|error| {
        let line = error.span().map(|span| line_at(text, span.start));
        (line, KeymapError::Syntax(error.message().to_owned()))
    })?;
    file.bind
        .into_iter()
        .map(|raw| {
            let line = line_at(text, raw.span().start);
            resolve(raw.into_inner())
                .map(|bind| Entry { bind, line })
                .map_err(|error| (Some(line), error))
        })
        .collect()
}

fn line_at(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(1, |before| before.matches('\n').count() + 1)
}

#[must_use]
pub fn normal(key: &str) -> Option<String> {
    let strokes: Vec<String> = key
        .split_whitespace()
        .map(|part| stroke(part).map(|stroke| stroke.unparse()))
        .collect::<Option<_>>()?;
    (!strokes.is_empty()).then(|| strokes.join(" "))
}

pub(super) fn stroke(part: &str) -> Option<Keystroke> {
    Keystroke::parse(part)
        .ok()
        .filter(|stroke| !stroke.key.is_empty())
}

fn resolve(raw: Raw) -> Result<Bind, KeymapError> {
    let Some(spec) = by_name(&raw.command) else {
        return Err(KeymapError::UnknownCommand {
            command: raw.command,
        });
    };
    let context = raw.context.unwrap_or_else(|| spec.context.to_owned());
    if !allowed(&spec, &context) {
        return Err(KeymapError::ContextNotAllowed {
            command: raw.command,
            context,
        });
    }
    if let Some(key) = raw
        .keys
        .iter()
        .find(|key| key.chars().any(char::is_uppercase))
    {
        return Err(KeymapError::UppercaseKey {
            command: raw.command,
            key: key.clone(),
        });
    }
    if let Some(key) = raw.keys.iter().find(|key| normal(key).is_none()) {
        return Err(KeymapError::BadKeystroke {
            command: raw.command,
            key: key.clone(),
        });
    }
    Ok(Bind {
        spec,
        context,
        keys: raw.keys,
    })
}

fn allowed(spec: &CommandSpec, context: &str) -> bool {
    if spec.context == GLOBAL || context == GLOBAL {
        return spec.context == context;
    }
    is_within(context, spec.context) && KeyBindingContextPredicate::parse(context).is_ok()
}

#[derive(Default)]
pub(super) struct Seen {
    entries: HashSet<(&'static str, String)>,
    keys: HashMap<(String, String), &'static str>,
}

impl Seen {
    pub(super) fn admit(&mut self, bind: Bind, names_conflicts: bool) -> Result<Bind, KeymapError> {
        if !self.entries.insert((bind.spec.name, bind.context.clone())) {
            return Err(KeymapError::DuplicateEntry {
                command: bind.spec.name.to_owned(),
                context: bind.context,
            });
        }
        for key in &bind.keys {
            let slot = (bind.context.clone(), normal(key).unwrap_or_default());
            let Some(first) = self.keys.insert(slot, bind.spec.name) else {
                continue;
            };
            if names_conflicts && first != bind.spec.name {
                return Err(KeymapError::KeyConflict {
                    key: key.clone(),
                    context: bind.context,
                    first: first.to_owned(),
                    second: bind.spec.name.to_owned(),
                });
            }
            return Err(KeymapError::DuplicateKey {
                key: key.clone(),
                context: bind.context,
            });
        }
        Ok(bind)
    }
}
