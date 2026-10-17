use std::collections::HashSet;
use std::fmt;

use gpui_kit::{KeyBindingContextPredicate, Keystroke};
use serde::Deserialize;

use super::Bind;
use crate::commands::{CommandSpec, GLOBAL, by_name, is_within};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum KeymapError {
    Syntax(String),
    UnknownCommand { command: String },
    BadKeystroke { command: String, key: String },
    ContextNotAllowed { command: String, context: String },
    DuplicateKey { key: String, context: String },
    DuplicateEntry { command: String, context: String },
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
        }
    }
}

impl std::error::Error for KeymapError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    bind: Vec<Raw>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    command: String,
    keys: Vec<String>,
    context: Option<String>,
}

pub(super) fn parse(text: &str) -> Result<Vec<Bind>, KeymapError> {
    let file: File =
        toml::from_str(text).map_err(|error| KeymapError::Syntax(error.to_string()))?;
    let mut seen = Seen::default();
    file.bind
        .into_iter()
        .map(|raw| resolve(raw).and_then(|bind| seen.admit(bind)))
        .collect()
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
struct Seen {
    entries: HashSet<(&'static str, String)>,
    keys: HashSet<(String, String)>,
}

impl Seen {
    fn admit(&mut self, bind: Bind) -> Result<Bind, KeymapError> {
        if !self.entries.insert((bind.spec.name, bind.context.clone())) {
            return Err(KeymapError::DuplicateEntry {
                command: bind.spec.name.to_owned(),
                context: bind.context,
            });
        }
        for key in &bind.keys {
            let normal = normal(key).unwrap_or_default();
            if !self.keys.insert((bind.context.clone(), normal)) {
                return Err(KeymapError::DuplicateKey {
                    key: key.clone(),
                    context: bind.context,
                });
            }
        }
        Ok(bind)
    }
}
