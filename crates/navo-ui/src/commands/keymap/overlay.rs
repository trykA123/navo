use std::fmt;

use super::parse::{self, Entry, Seen, normal};
use super::{Bind, Keymap, KeymapError};
use crate::commands::{LAYOUT_MODE, is_fixed_digit};

mod legacy;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Overlaid {
    pub keymap: Keymap,
    pub warnings: Vec<KeymapWarning>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum KeymapWarning {
    /// A user key took a built-in key from another command.
    Taken {
        line: usize,
        key: String,
        context: String,
        kept: String,
        dropped: String,
    },
    /// An old user entry changed meaning with a newer built-in keymap.
    Legacy { line: usize, text: String },
    /// A user key for a layout mode verb was a bare digit, which always picks a numbered layout.
    Fixed {
        line: usize,
        key: String,
        command: String,
    },
}

impl KeymapWarning {
    #[must_use]
    pub fn line(&self) -> usize {
        match self {
            Self::Taken { line, .. } | Self::Legacy { line, .. } | Self::Fixed { line, .. } => {
                *line
            }
        }
    }
}

impl fmt::Display for KeymapWarning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Taken {
                line,
                key,
                context,
                kept,
                dropped,
            } => write!(
                formatter,
                "line {line}: `{key}` now runs `{kept}` instead of `{dropped}` in `{context}`"
            ),
            Self::Legacy { line, text } => write!(formatter, "line {line}: {text}"),
            Self::Fixed { line, key, command } => write!(
                formatter,
                "line {line}: `{key}` always picks a numbered layout in layout mode, so `{command}` cannot use it"
            ),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UserKeymapError {
    pub line: Option<usize>,
    pub error: KeymapError,
}

impl fmt::Display for UserKeymapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "line {line}: {}", self.error),
            None => write!(formatter, "{}", self.error),
        }
    }
}

impl std::error::Error for UserKeymapError {}

pub(super) fn overlay(base: &Keymap, text: &str) -> Result<Overlaid, UserKeymapError> {
    let (user, mut warnings) = validated(text)?;
    warn_fixed_digits(&user, &mut warnings);
    let mut binds = base.binds.clone();
    for entry in &user {
        match binds.iter_mut().find(|bind| same_slot(bind, &entry.bind)) {
            Some(bind) => bind.keys.clone_from(&entry.bind.keys),
            None => binds.push(entry.bind.clone()),
        }
    }
    for bind in &mut binds {
        if !user.iter().any(|entry| same_slot(bind, &entry.bind)) {
            drop_taken(bind, &user, &mut warnings);
        }
    }
    warnings.sort_by_key(KeymapWarning::line);
    let changed = user
        .iter()
        .filter(|entry| is_change(base, &entry.bind))
        .map(|entry| entry.bind.spec.name)
        .collect();
    Ok(Overlaid {
        keymap: Keymap {
            binds,
            user: user
                .iter()
                .map(|entry| (entry.bind.spec.name, entry.bind.context.clone()))
                .collect(),
            changed,
        },
        warnings,
    })
}

/// Warns about a bare digit 1-9 on a layout verb; it stays in the keymap so a rewrite keeps it.
fn warn_fixed_digits(user: &[Entry], warnings: &mut Vec<KeymapWarning>) {
    for entry in user
        .iter()
        .filter(|entry| entry.bind.context == LAYOUT_MODE)
    {
        let fixed = entry
            .bind
            .keys
            .iter()
            .filter(|key| normal(key).is_some_and(|key| is_fixed_digit(&key)));
        warnings.extend(fixed.map(|key| KeymapWarning::Fixed {
            line: entry.line,
            key: key.clone(),
            command: entry.bind.spec.name.to_owned(),
        }));
    }
}

fn key_set(keys: &[String]) -> Vec<String> {
    let mut set: Vec<String> = keys.iter().filter_map(|key| normal(key)).collect();
    set.sort();
    set.dedup();
    set
}

fn is_change(base: &Keymap, mine: &Bind) -> bool {
    let before = base
        .binds
        .iter()
        .find(|bind| same_slot(bind, mine))
        .map_or_else(Vec::new, |bind| key_set(&bind.keys));
    before != key_set(&mine.keys)
}

/// The user's entries on the current slots, and the warnings from moving them.
fn validated(text: &str) -> Result<(Vec<Entry>, Vec<KeymapWarning>), UserKeymapError> {
    let entries = parse::entries(text).map_err(|(line, error)| UserKeymapError { line, error })?;
    admit_all(&entries)?;
    let (entries, warnings) = legacy::migrate(entries);
    admit_all(&entries)?;
    Ok((entries, warnings))
}

fn admit_all(entries: &[Entry]) -> Result<(), UserKeymapError> {
    let mut seen = Seen::default();
    for entry in entries {
        seen.admit(entry.bind.clone(), true)
            .map_err(|error| UserKeymapError {
                line: Some(entry.line),
                error,
            })?;
    }
    Ok(())
}

fn same_slot(one: &Bind, other: &Bind) -> bool {
    one.spec.name == other.spec.name && one.context == other.context
}

fn drop_taken(bind: &mut Bind, user: &[Entry], warnings: &mut Vec<KeymapWarning>) {
    let mut kept = Vec::new();
    for key in std::mem::take(&mut bind.keys) {
        match taken_by(user, bind, &key) {
            Some(warning) => warnings.push(warning),
            None => kept.push(key),
        }
    }
    bind.keys = kept;
}

fn taken_by(user: &[Entry], bind: &Bind, key: &str) -> Option<KeymapWarning> {
    let wanted = normal(key)?;
    let entry = user.iter().find(|entry| {
        entry.bind.context == bind.context
            && entry
                .bind
                .keys
                .iter()
                .any(|mine| normal(mine).as_ref() == Some(&wanted))
    })?;
    Some(KeymapWarning::Taken {
        line: entry.line,
        key: key.to_owned(),
        context: bind.context.clone(),
        kept: entry.bind.spec.name.to_owned(),
        dropped: bind.spec.name.to_owned(),
    })
}
