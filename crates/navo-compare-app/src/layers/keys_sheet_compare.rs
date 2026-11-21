//! The Keys sheet's File and Stream groups, built from the compare key file in `sidewise-ui`
//! (NC114), so the sheet lists the keys the compare screen really binds.

use sidewise_kit::prelude::SharedString;
use sidewise_ui::compare_keys::{COMPARE_LABELS, CompareKey, compare_keys};

use super::keys_sheet::{KeyGroup, KeyRow};

/// The runtime groups for the compare screen; empty when the key file does not parse.
pub fn compare_groups() -> Vec<KeyGroup> {
    compare_keys()
        .map(|keys| groups_from(&keys, COMPARE_LABELS, translated))
        .unwrap_or_default()
}

/// The Fluent text of `id`, or `None` when the catalog has no such message (no raw id shown).
pub fn translated(id: &str) -> Option<String> {
    sidewise_l10n::catalog().lookup(id, None).ok()
}

/// Groups `keys` by context in first-seen order; commands without a label or text are skipped.
pub fn groups_from(
    keys: &[CompareKey],
    labels: &[(&str, &str)],
    label_of: impl Fn(&str) -> Option<String>,
) -> Vec<KeyGroup> {
    let mut groups: Vec<KeyGroup> = Vec::new();
    for key in keys {
        let Some((_, id)) = labels.iter().find(|(command, _)| *command == key.command) else {
            continue;
        };
        let Some(text) = label_of(id) else {
            continue;
        };
        let row = KeyRow {
            label: text.into(),
            keys: key.keys.clone(),
        };
        let title = title_of(&key.context);
        match groups.iter_mut().find(|group| group.title == title) {
            Some(group) => group.rows.push(row),
            None => groups.push(KeyGroup {
                title,
                rows: vec![row],
            }),
        }
    }
    groups
}

/// The group title of a key context: File, Stream, Find results, or the context without its
/// "Compare" prefix, so a raw context id never shows.
fn title_of(context: &str) -> SharedString {
    match context {
        "CompareView" => "File".into(),
        "CompareStream" => "Stream".into(),
        "CompareOutline" => "Find results".into(),
        other => match other.strip_prefix("Compare") {
            Some(rest) if !rest.is_empty() => rest.to_owned().into(),
            _ => other.to_owned().into(),
        },
    }
}
