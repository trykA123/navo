use std::fmt;

use super::parse::{self, Entry, Seen, normal};
use super::{Bind, Keymap, KeymapError};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Overlaid {
    pub keymap: Keymap,
    pub warnings: Vec<KeymapWarning>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeymapWarning {
    pub line: usize,
    pub key: String,
    pub context: String,
    pub kept: String,
    pub dropped: String,
}

impl fmt::Display for KeymapWarning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            line,
            key,
            context,
            kept,
            dropped,
        } = self;
        write!(
            formatter,
            "line {line}: `{key}` now runs `{kept}` instead of `{dropped}` in `{context}`"
        )
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
    let user = validated(text)?;
    let mut binds = base.binds.clone();
    for entry in &user {
        match binds.iter_mut().find(|bind| same_slot(bind, &entry.bind)) {
            Some(bind) => bind.keys.clone_from(&entry.bind.keys),
            None => binds.push(entry.bind.clone()),
        }
    }
    let mut warnings = Vec::new();
    for bind in &mut binds {
        if !user.iter().any(|entry| same_slot(bind, &entry.bind)) {
            drop_taken(bind, &user, &mut warnings);
        }
    }
    warnings.sort_by_key(|warning| warning.line);
    Ok(Overlaid {
        keymap: Keymap { binds },
        warnings,
    })
}

fn validated(text: &str) -> Result<Vec<Entry>, UserKeymapError> {
    let located = |(line, error)| UserKeymapError { line, error };
    let entries = parse::entries(text).map_err(located)?;
    let mut seen = Seen::default();
    for entry in &entries {
        seen.admit(entry.bind.clone(), true)
            .map_err(|error| located((Some(entry.line), error)))?;
    }
    Ok(entries)
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
    Some(KeymapWarning {
        line: entry.line,
        key: key.to_owned(),
        context: bind.context.clone(),
        kept: entry.bind.spec.name.to_owned(),
        dropped: bind.spec.name.to_owned(),
    })
}
