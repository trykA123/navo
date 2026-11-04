//! Carries a user's overrides of slots the built-in keymap no longer has over to the slots that replace them.

use super::{KeymapWarning, key_set};
use crate::commands::keymap::Bind;
use crate::commands::keymap::parse::{Entry, normal};

const VIEWER: &str = "Viewer";
const BACK: &str = "viewer.back";
const CLOSE: &str = "viewer.close";
const MARK_PREFIX: &str = "mark.";

/// Contexts that once held their own `viewer.back`; the one `Viewer` slot replaces them.
/// Their overrides move only when all of them are set and agree.
const OLD_BACK: [&str; 11] = [
    "ViewerGoTo",
    "Paged",
    "ViewerFind",
    "Tree",
    "Image",
    "Pdf",
    "MemoryImage",
    "Elf",
    "Map",
    "CanTrace",
    "Mdf",
];

/// Where the marks were bound (`Viewer`) and the slots that hold them now.
const MARKS_NOW: [&str; 2] = ["Paged", "Editor"];

/// Rewrites old user entries onto the current slots and warns where a behaviour would change.
pub(super) fn migrate(entries: Vec<Entry>) -> (Vec<Entry>, Vec<KeymapWarning>) {
    let had_back = entries.iter().any(|entry| entry.bind.spec.name == BACK);
    let mut warnings = Vec::new();
    let entries = marks(entries);
    let entries = back(entries, &mut warnings);
    warnings.extend(close_warning(&entries, had_back));
    (entries, warnings)
}

fn is_old_mark(bind: &Bind) -> bool {
    bind.spec.name.starts_with(MARK_PREFIX) && bind.context == VIEWER
}

fn marks(entries: Vec<Entry>) -> Vec<Entry> {
    let slots: Vec<(&str, String)> = entries
        .iter()
        .map(|entry| (entry.bind.spec.name, entry.bind.context.clone()))
        .collect();
    let mut moved = Vec::new();
    for entry in entries {
        if !is_old_mark(&entry.bind) {
            moved.push(entry);
            continue;
        }
        for context in MARKS_NOW {
            let explicit = slots
                .iter()
                .any(|(name, at)| *name == entry.bind.spec.name && at == context);
            if !explicit {
                moved.push(in_context(&entry, context));
            }
        }
    }
    moved
}

fn in_context(entry: &Entry, context: &str) -> Entry {
    Entry {
        bind: Bind {
            context: context.to_owned(),
            ..entry.bind.clone()
        },
        line: entry.line,
    }
}

fn is_old_back(entry: &Entry) -> bool {
    entry.bind.spec.name == BACK && OLD_BACK.contains(&entry.bind.context.as_str())
}

fn back(entries: Vec<Entry>, warnings: &mut Vec<KeymapWarning>) -> Vec<Entry> {
    let (old, mut kept): (Vec<Entry>, Vec<Entry>) = entries.into_iter().partition(is_old_back);
    let explicit = kept
        .iter()
        .any(|entry| entry.bind.spec.name == BACK && entry.bind.context == VIEWER);
    let Some(first) = old.first() else {
        return kept;
    };
    let wanted = key_set(&first.bind.keys);
    let every_context = OLD_BACK
        .iter()
        .all(|context| old.iter().any(|entry| entry.bind.context == *context));
    let agree = every_context && old.iter().all(|entry| key_set(&entry.bind.keys) == wanted);
    if !agree {
        warnings.push(disagreement(&old));
    } else if !explicit {
        kept.push(in_context(first, VIEWER));
    }
    kept
}

fn disagreement(old: &[Entry]) -> KeymapWarning {
    let places: Vec<String> = old
        .iter()
        .map(|entry| format!("`{}` (line {})", entry.bind.context, entry.line))
        .collect();
    KeymapWarning::Legacy {
        line: old.iter().map(|entry| entry.line).min().unwrap_or(1),
        text: format!(
            "`{BACK}` is set in {}: per-part back keys are gone, and `{BACK}` in `{VIEWER}` now applies to the whole viewer, so these are ignored",
            places.join(", ")
        ),
    }
}

fn close_warning(entries: &[Entry], had_back: bool) -> Option<KeymapWarning> {
    let escape = normal("escape");
    let entry = entries
        .iter()
        .find(|entry| entry.bind.spec.name == CLOSE && entry.bind.context == VIEWER)?;
    let keeps_escape = entry.bind.keys.iter().any(|key| normal(key) == escape);
    (!had_back && !keeps_escape).then(|| KeymapWarning::Legacy {
        line: entry.line,
        text: format!(
            "`{CLOSE}` no longer has Esc: Esc now steps back, and closes the viewer when nothing is left. Set `{BACK}` to change that"
        ),
    })
}

#[cfg(test)]
#[path = "legacy_tests.rs"]
mod tests;
