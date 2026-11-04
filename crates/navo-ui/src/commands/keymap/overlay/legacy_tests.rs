use super::{KeymapWarning, OLD_BACK, migrate};
use crate::commands::keymap::parse::{Entry, entries};

fn parsed(text: &str) -> Vec<Entry> {
    entries(text).unwrap_or_else(|(_, error)| panic!("{error}"))
}

fn bind(command: &str, keys: &str, context: &str) -> String {
    format!("[[bind]]\ncommand = \"{command}\"\nkeys = [{keys}]\ncontext = \"{context}\"\n\n")
}

fn slots(entries: &[Entry]) -> Vec<(String, String, Vec<String>)> {
    let mut found: Vec<_> = entries
        .iter()
        .map(|entry| {
            let bind = &entry.bind;
            (
                bind.spec.name.to_owned(),
                bind.context.clone(),
                bind.keys.clone(),
            )
        })
        .collect();
    found.sort();
    found
}

fn slot(command: &str, context: &str, keys: &[&str]) -> (String, String, Vec<String>) {
    (
        command.to_owned(),
        context.to_owned(),
        keys.iter().map(|&key| key.to_owned()).collect(),
    )
}

#[test]
fn a_mark_in_viewer_becomes_one_entry_per_new_slot() {
    let (moved, warnings) = migrate(parsed(&bind("mark.clear", "\"ctrl-9\"", "Viewer")));
    assert_eq!(
        slots(&moved),
        [
            slot("mark.clear", "Editor", &["ctrl-9"]),
            slot("mark.clear", "Paged", &["ctrl-9"]),
        ]
    );
    assert_eq!(warnings, []);
}

#[test]
fn a_mark_in_a_new_slot_is_left_alone_and_wins_over_the_old_one() {
    let text = bind("mark.clear", "\"ctrl-9\"", "Viewer") + &bind("mark.clear", "", "Editor");
    let (moved, _) = migrate(parsed(&text));
    assert_eq!(
        slots(&moved),
        [
            slot("mark.clear", "Editor", &[]),
            slot("mark.clear", "Paged", &["ctrl-9"]),
        ]
    );
}

#[test]
fn other_entries_pass_through() {
    let text =
        bind("files.copy", "\"f5\"", "FileList") + &bind("bookmark.next", "\"f9\"", "Viewer");
    let (moved, warnings) = migrate(parsed(&text));
    assert_eq!(
        slots(&moved),
        [
            slot("bookmark.next", "Viewer", &["f9"]),
            slot("files.copy", "FileList", &["f5"]),
        ]
    );
    assert_eq!(warnings, []);
}

#[test]
fn back_entries_in_every_old_context_collapse_whatever_the_key_order() {
    let text: String = OLD_BACK
        .iter()
        .enumerate()
        .map(|(index, context)| {
            let keys = if index % 2 == 0 {
                "\"f9\", \"f10\""
            } else {
                "\"f10\", \"f9\""
            };
            bind("viewer.back", keys, context)
        })
        .collect();
    let (moved, warnings) = migrate(parsed(&text));
    assert_eq!(
        slots(&moved),
        [slot("viewer.back", "Viewer", &["f9", "f10"])]
    );
    assert_eq!(warnings, []);
}

#[test]
fn agreeing_back_entries_in_only_some_contexts_vanish_with_a_warning() {
    let text = bind("viewer.back", "\"f9\"", "Tree") + &bind("viewer.back", "\"f9\"", "Pdf");
    let (moved, warnings) = migrate(parsed(&text));
    assert_eq!(slots(&moved), []);
    assert!(matches!(
        warnings.as_slice(),
        [KeymapWarning::Legacy { line: 1, .. }]
    ));
}

#[test]
fn disagreeing_back_entries_vanish_and_the_warning_names_them_and_their_lines() {
    let text = bind("viewer.back", "\"f9\"", "Tree") + &bind("viewer.back", "", "Mdf");
    let (moved, warnings) = migrate(parsed(&text));
    assert_eq!(slots(&moved), []);
    let [KeymapWarning::Legacy { line, text }] = warnings.as_slice() else {
        panic!("{warnings:?}");
    };
    assert_eq!(*line, 1);
    assert!(
        text.contains("`Tree` (line 1)") && text.contains("`Mdf` (line 6)"),
        "{text}"
    );
}

#[test]
fn a_viewer_back_entry_wins_over_disagreeing_old_ones_with_a_warning() {
    let text = bind("viewer.back", "\"f9\"", "Viewer") + &bind("viewer.back", "\"f10\"", "Tree");
    let (moved, warnings) = migrate(parsed(&text));
    assert_eq!(slots(&moved), [slot("viewer.back", "Viewer", &["f9"])]);
    assert!(matches!(
        warnings.as_slice(),
        [KeymapWarning::Legacy { line: 6, .. }]
    ));
}

#[test]
fn a_viewer_back_entry_with_partial_old_entries_warns() {
    let text = bind("viewer.back", "\"f9\"", "Viewer") + &bind("viewer.back", "\"f9\"", "Tree");
    let (moved, warnings) = migrate(parsed(&text));
    assert_eq!(slots(&moved), [slot("viewer.back", "Viewer", &["f9"])]);
    assert_eq!(warnings.len(), 1);
}

#[test]
fn a_viewer_back_entry_with_all_old_entries_agreeing_does_not_warn() {
    let text: String = OLD_BACK
        .iter()
        .map(|context| bind("viewer.back", "\"f9\"", context))
        .chain([bind("viewer.back", "\"f9\"", "Viewer")])
        .collect();
    let (moved, warnings) = migrate(parsed(&text));
    assert_eq!(slots(&moved), [slot("viewer.back", "Viewer", &["f9"])]);
    assert_eq!(warnings, []);
}

#[test]
fn a_close_override_without_escape_warns_unless_back_is_set() {
    let close = bind("viewer.close", "\"f8\"", "Viewer");
    let (moved, warnings) = migrate(parsed(&close));
    assert_eq!(slots(&moved), [slot("viewer.close", "Viewer", &["f8"])]);
    assert!(matches!(
        warnings.as_slice(),
        [KeymapWarning::Legacy { line: 1, .. }]
    ));
    let with_back = close + &bind("viewer.back", "", "Viewer");
    assert_eq!(migrate(parsed(&with_back)).1, []);
}

#[test]
fn a_close_override_that_keeps_escape_does_not_warn() {
    let text = bind("viewer.close", "\"f8\", \"escape\"", "Viewer");
    assert_eq!(migrate(parsed(&text)).1, []);
}

#[test]
fn a_close_override_in_an_inner_context_does_not_warn() {
    let text = bind("viewer.close", "\"f8\"", "Tree");
    assert_eq!(migrate(parsed(&text)).1, []);
}
