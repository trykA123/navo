use std::collections::BTreeMap;

use super::{Settings, SettingsError};

mod plain;
mod steps;
mod tree;
mod unused;

pub use steps::{complete_text, upgrade_text};
#[cfg(test)]
pub use unused::UNUSED_NOTE;
pub use unused::mark_unused;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultChange {
    pub key: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    pub text: String,
    pub written: Vec<(String, String)>,
    pub changes: Vec<DefaultChange>,
}

pub type Ledger = BTreeMap<String, String>;

pub fn reconcile(
    text: &str,
    ledger: &Ledger,
    defaults: &Settings,
) -> Result<Reconciled, SettingsError> {
    let upgraded = upgrade_text(text, ledger, defaults)?;
    let completed = complete_text(&upgraded.text, defaults)?;
    let noted = mark_unused(&completed.text)?;
    let mut written = upgraded.written;
    written.extend(completed.written);
    let done = Reconciled {
        text: noted,
        written,
        changes: upgraded.changes,
    };
    if is_safe(text, &done)? {
        Ok(done)
    } else {
        Err(SettingsError::Refused)
    }
}

fn is_safe(before: &str, done: &Reconciled) -> Result<bool, SettingsError> {
    let (before, after) = (tree::leaves(before)?, tree::leaves(&done.text)?);
    let rewritten = |path: &[String]| {
        done.changes
            .iter()
            .any(|change| change.key == path.join("."))
    };
    let kept = before
        .iter()
        .all(|(path, value)| after.get(path) == Some(value) || rewritten(path));
    let landed = done.written.iter().all(|(key, value)| {
        let path: Vec<String> = key.split('.').map(str::to_owned).collect();
        after.get(&path) == Some(value)
    });
    Ok(kept && landed && super::parse(&done.text).is_ok())
}
