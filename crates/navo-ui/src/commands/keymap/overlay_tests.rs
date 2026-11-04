use super::{Keymap, KeymapError, KeymapWarning, Overlaid, UserKeymapError};

fn one(command: &str, keys: &str, context: Option<&str>) -> String {
    let context = context.map_or_else(String::new, |context| format!("context = \"{context}\"\n"));
    format!("[[bind]]\ncommand = \"{command}\"\nkeys = [{keys}]\n{context}\n")
}

fn overlaid(text: &str) -> Overlaid {
    Keymap::builtin().overlay(text).expect("valid user keymap")
}

fn error_of(text: &str) -> Option<UserKeymapError> {
    Keymap::builtin().overlay(text).err()
}

fn keys(keymap: &Keymap, command: &str) -> Vec<String> {
    keymap.keys_of(command).map(str::to_owned).collect()
}

fn keys_in(keymap: &Keymap, command: &str, context: &str) -> Option<Vec<String>> {
    keymap
        .binds()
        .iter()
        .find(|bind| bind.spec.name == command && bind.context == context)
        .map(|bind| bind.keys.clone())
}

#[test]
fn an_empty_user_file_changes_nothing() {
    let result = overlaid("# nothing yet\n");
    assert_eq!(&result.keymap, Keymap::builtin());
    assert_eq!(result.warnings, []);
}

#[test]
fn a_user_entry_replaces_the_built_in_keys_of_that_command() {
    let result = overlaid(&one("files.copy", "\"alt-f5\", \"alt-c\"", None));
    assert_eq!(keys(&result.keymap, "files.copy"), ["alt-f5", "alt-c"]);
    assert_eq!(keys(&result.keymap, "files.move"), ["f6"]);
    assert_eq!(result.keymap.binds().len(), Keymap::builtin().binds().len());
    assert_eq!(result.warnings, []);
}

#[test]
fn a_user_entry_replaces_only_its_own_context() {
    let result = overlaid(&one("viewer.go_to_end", "\"backspace\"", Some("Tree")));
    let keymap = &result.keymap;
    assert_eq!(
        keys_in(keymap, "viewer.go_to_end", "Tree"),
        Some(vec!["backspace".to_owned()])
    );
    assert_eq!(
        keys_in(keymap, "viewer.go_to_end", "Archive"),
        Some(vec!["end".to_owned()])
    );
}

#[test]
fn an_empty_key_list_unbinds_the_command() {
    let result = overlaid(&one("files.copy", "", None));
    assert_eq!(result.keymap.keys_of("files.copy").count(), 0);
    assert!(!result.keymap.has_key("files.copy", "f5"));
}

#[test]
fn an_allowed_context_missing_from_the_built_in_file_adds_a_binding() {
    assert_eq!(
        keys_in(Keymap::builtin(), "viewer.go_to_end", "Table"),
        None
    );
    let result = overlaid(&one("viewer.go_to_end", "\"backspace\"", Some("Table")));
    let keymap = &result.keymap;
    assert_eq!(
        keys_in(keymap, "viewer.go_to_end", "Table"),
        Some(vec!["backspace".to_owned()])
    );
    assert_eq!(
        keys_in(keymap, "viewer.go_to_end", "Tree"),
        Some(vec!["end".to_owned()])
    );
    assert_eq!(keymap.binds().len(), Keymap::builtin().binds().len() + 1);
}

#[test]
fn a_user_key_taken_from_another_command_wins_with_a_warning() {
    let text = "# my keys\n\n".to_owned() + &one("files.sort_by_size", "\"ctrl-f3\"", None);
    let result = overlaid(&text);
    let keymap = &result.keymap;
    assert!(keymap.has_key("files.sort_by_size", "ctrl-f3"));
    assert!(!keymap.has_key("files.sort_by_size", "ctrl-f6"));
    assert_eq!(keys(keymap, "files.sort_by_name"), Vec::<String>::new());
    assert_eq!(
        result.warnings,
        [KeymapWarning::Taken {
            line: 3,
            key: "ctrl-f3".to_owned(),
            context: "FileList".to_owned(),
            kept: "files.sort_by_size".to_owned(),
            dropped: "files.sort_by_name".to_owned(),
        }]
    );
    let shown = result.warnings[0].to_string();
    assert!(
        shown.contains("files.sort_by_size") && shown.contains("files.sort_by_name"),
        "{shown}"
    );
}

#[test]
fn a_taken_key_drops_only_that_key_of_the_other_command() {
    let result = overlaid(&one("files.copy", "\"delete\"", None));
    assert_eq!(keys(&result.keymap, "files.recycle"), ["f8"]);
    assert_eq!(result.warnings.len(), 1);
}

#[test]
fn a_key_taken_from_a_command_the_user_also_rebinds_is_no_conflict() {
    let text = one("files.copy", "\"f6\"", None) + &one("files.move", "\"f5\"", None);
    let result = overlaid(&text);
    assert_eq!(keys(&result.keymap, "files.copy"), ["f6"]);
    assert_eq!(keys(&result.keymap, "files.move"), ["f5"]);
    assert_eq!(result.warnings, []);
}

#[test]
fn a_built_in_key_in_another_context_is_no_conflict() {
    let result = overlaid(&one("viewer.go_to_end", "\"f5\"", Some("Tree")));
    assert_eq!(keys(&result.keymap, "files.copy"), ["f5"]);
    assert_eq!(result.warnings, []);
}

#[test]
fn two_user_entries_with_one_key_in_one_context_are_an_error() {
    let text = one("files.copy", "\"alt-x\"", None) + &one("files.move", "\"alt-x\"", None);
    assert_eq!(
        error_of(&text),
        Some(UserKeymapError {
            line: Some(5),
            error: KeymapError::KeyConflict {
                key: "alt-x".to_owned(),
                context: "FileList".to_owned(),
                first: "files.copy".to_owned(),
                second: "files.move".to_owned(),
            },
        })
    );
}

#[test]
fn an_upper_case_key_is_an_error() {
    for key in ["F5", "Ctrl-x", "ctrl-shift-X"] {
        assert_eq!(
            error_of(&one("files.copy", &format!("\"{key}\""), None)),
            Some(UserKeymapError {
                line: Some(1),
                error: KeymapError::UppercaseKey {
                    command: "files.copy".to_owned(),
                    key: key.to_owned(),
                },
            }),
            "{key}"
        );
    }
    let message = KeymapError::UppercaseKey {
        command: "files.copy".to_owned(),
        key: "F5".to_owned(),
    }
    .to_string();
    assert!(message.contains("f5"), "{message}");
}

#[test]
fn an_unknown_command_is_an_error_on_its_line() {
    let text = one("files.copy", "\"f5\"", None) + &one("files.teleport", "\"f9\"", None);
    let error = error_of(&text).expect("error");
    assert_eq!(
        error,
        UserKeymapError {
            line: Some(5),
            error: KeymapError::UnknownCommand {
                command: "files.teleport".to_owned()
            },
        }
    );
    assert!(error.to_string().starts_with("line 5: "), "{error}");
}

#[test]
fn a_user_entry_for_a_removed_preview_toggle_is_an_unknown_command() {
    for command in ["pane.toggle_preview", "preview.toggle"] {
        assert_eq!(
            error_of(&one(command, "\"ctrl-q\"", None)),
            Some(UserKeymapError {
                line: Some(1),
                error: KeymapError::UnknownCommand {
                    command: command.to_owned()
                },
            })
        );
    }
}

#[test]
fn a_context_outside_the_home_context_is_an_error() {
    assert_eq!(
        error_of(&one("tab.next", "\"f12\"", Some("Tree"))),
        Some(UserKeymapError {
            line: Some(1),
            error: KeymapError::ContextNotAllowed {
                command: "tab.next".to_owned(),
                context: "Tree".to_owned(),
            },
        })
    );
}

#[test]
fn a_bad_keystroke_and_a_repeated_entry_are_errors() {
    let bad = one("files.copy", "\"ctrl-a-b\"", None);
    assert!(matches!(
        error_of(&bad).map(|error| error.error),
        Some(KeymapError::BadKeystroke { .. })
    ));
    let twice = one("files.copy", "\"f5\"", None) + &one("files.copy", "\"alt-c\"", None);
    assert!(matches!(
        error_of(&twice).map(|error| error.error),
        Some(KeymapError::DuplicateEntry { .. })
    ));
}

#[test]
fn broken_toml_is_a_syntax_error_with_its_line() {
    let error = error_of("\n\n[[bind]\n").expect("error");
    assert!(matches!(error.error, KeymapError::Syntax(_)), "{error:?}");
    assert_eq!(error.line, Some(3));
}

#[test]
fn the_built_in_keymap_has_no_upper_case_key() {
    let upper: Vec<&str> = Keymap::builtin()
        .binds()
        .iter()
        .flat_map(|bind| bind.keys.iter().map(String::as_str))
        .filter(|key| key.chars().any(char::is_uppercase))
        .collect();
    assert_eq!(upper, Vec::<&str>::new());
}

#[test]
fn the_overlay_remembers_which_slots_the_user_set() {
    let text = format!(
        "{}{}",
        one("files.copy", "\"alt-f5\"", None),
        one("viewer.go_to_end", "\"backspace\"", Some("Tree"))
    );
    let keymap = overlaid(&text).keymap;
    assert!(keymap.is_user("files.copy", "FileList"));
    assert!(keymap.is_user("viewer.go_to_end", "Tree"));
    assert!(!keymap.is_user("viewer.go_to_end", "Paged"));
    assert!(!keymap.is_user("files.move", "FileList"));
    assert!(keymap.is_changed("viewer.go_to_end"));
    assert!(!keymap.is_changed("files.move"));
}

#[test]
fn the_built_in_keymap_has_no_user_slots() {
    let keymap = Keymap::builtin();
    assert!(!keymap.is_changed("files.copy"));
    assert!(!keymap.is_user("files.copy", "FileList"));
}

#[test]
fn an_old_viewer_mark_override_replaces_the_keys_in_paged_and_editor() {
    let result = overlaid(&one("mark.colour_three", "\"ctrl-9\"", Some("Viewer")));
    for context in ["Paged", "Editor"] {
        assert_eq!(
            keys_in(&result.keymap, "mark.colour_three", context),
            Some(vec!["ctrl-9".to_owned()]),
            "{context}"
        );
        assert!(result.keymap.is_user("mark.colour_three", context));
    }
    assert_eq!(result.warnings, []);
}

#[test]
fn an_old_viewer_mark_override_without_context_and_with_no_keys_unbinds_both() {
    let result = overlaid(&one("mark.colour_three", "", None));
    for context in ["Paged", "Editor"] {
        assert_eq!(
            keys_in(&result.keymap, "mark.colour_three", context),
            Some(vec![])
        );
    }
    assert_eq!(
        keys_in(&result.keymap, "mark.colour_two", "Paged"),
        Some(vec!["ctrl-2".to_owned()])
    );
}

#[test]
fn an_explicit_mark_entry_in_a_new_slot_beats_the_old_viewer_one() {
    let text = one("mark.colour_three", "\"ctrl-9\"", Some("Viewer"))
        + &one("mark.colour_three", "\"ctrl-8\"", Some("Editor"));
    let result = overlaid(&text);
    assert_eq!(
        keys_in(&result.keymap, "mark.colour_three", "Editor"),
        Some(vec!["ctrl-8".to_owned()])
    );
    assert_eq!(
        keys_in(&result.keymap, "mark.colour_three", "Paged"),
        Some(vec!["ctrl-9".to_owned()])
    );
}

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

fn every_old_back(keys: &str) -> String {
    OLD_BACK
        .iter()
        .map(|context| one("viewer.back", keys, Some(context)))
        .collect()
}

#[test]
fn old_inner_back_overrides_that_all_agree_move_to_the_viewer_slot() {
    let result = overlaid(&every_old_back("\"f9\""));
    assert_eq!(
        keys_in(&result.keymap, "viewer.back", "Viewer"),
        Some(vec!["f9".to_owned()])
    );
    assert_eq!(keys_in(&result.keymap, "viewer.back", "Tree"), None);
    assert!(result.keymap.is_user("viewer.back", "Viewer"));
    assert_eq!(result.warnings, []);
}

#[test]
fn old_empty_back_overrides_in_every_context_unbind_the_outer_escape() {
    let result = overlaid(&every_old_back(""));
    assert_eq!(
        keys_in(&result.keymap, "viewer.back", "Viewer"),
        Some(vec![])
    );
}

#[test]
fn a_partial_set_of_old_back_overrides_is_not_migrated_and_warns() {
    let result = overlaid(&one("viewer.back", "", Some("Tree")));
    assert_eq!(
        keys_in(&result.keymap, "viewer.back", "Viewer"),
        Some(vec!["escape".to_owned()])
    );
    assert_eq!(keys_in(&result.keymap, "viewer.back", "Tree"), None);
    assert_eq!(result.warnings.len(), 1);
    let shown = result.warnings[0].to_string();
    assert!(
        shown.contains("Tree") && shown.contains("whole viewer"),
        "{shown}"
    );
}

#[test]
fn old_inner_back_overrides_that_disagree_are_dropped_with_a_warning() {
    let text =
        one("viewer.back", "\"backspace\"", Some("Tree")) + &one("viewer.back", "", Some("Paged"));
    let result = overlaid(&text);
    assert_eq!(
        keys_in(&result.keymap, "viewer.back", "Viewer"),
        Some(vec!["escape".to_owned()])
    );
    assert_eq!(keys_in(&result.keymap, "viewer.back", "Tree"), None);
    assert_eq!(result.warnings.len(), 1);
    let shown = result.warnings[0].to_string();
    assert!(
        shown.contains("Tree") && shown.contains("Paged") && shown.contains("viewer.back"),
        "{shown}"
    );
}

#[test]
fn an_explicit_viewer_back_entry_beats_the_old_inner_ones() {
    let text = one("viewer.back", "\"f9\"", Some("Viewer"))
        + &one("viewer.back", "\"backspace\"", Some("Tree"));
    let result = overlaid(&text);
    assert_eq!(
        keys_in(&result.keymap, "viewer.back", "Viewer"),
        Some(vec!["f9".to_owned()])
    );
}

#[test]
fn a_viewer_close_override_without_escape_warns_that_escape_now_steps_back() {
    let result = overlaid(&one("viewer.close", "\"f8\"", Some("Viewer")));
    assert_eq!(keys(&result.keymap, "viewer.close"), ["f8"]);
    assert_eq!(result.warnings.len(), 1);
    let shown = result.warnings[0].to_string();
    assert!(
        shown.contains("viewer.close") && shown.contains("viewer.back"),
        "{shown}"
    );
}

#[test]
fn a_viewer_close_override_with_a_back_override_or_with_escape_stays_quiet() {
    let with_back = one("viewer.close", "\"f8\"", None) + &one("viewer.back", "", Some("Viewer"));
    assert_eq!(overlaid(&with_back).warnings, []);
    let with_escape = overlaid(&one("viewer.close", "\"escape\"", None));
    assert_eq!(keys(&with_escape.keymap, "viewer.close"), ["escape"]);
    assert_eq!(
        keys(&with_escape.keymap, "viewer.back"),
        Vec::<String>::new()
    );
}
