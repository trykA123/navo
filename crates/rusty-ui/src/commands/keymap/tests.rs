use super::{BUILTIN, Keymap, KeymapError, shown};

fn one(command: &str, keys: &str, context: Option<&str>) -> String {
    let context = context.map_or_else(String::new, |context| format!("context = \"{context}\"\n"));
    format!("[[bind]]\ncommand = \"{command}\"\nkeys = [{keys}]\n{context}\n")
}

fn error_of(text: &str) -> Option<KeymapError> {
    Keymap::parse(text).err()
}

#[test]
fn the_built_in_keymap_parses_and_validates() {
    let parsed = Keymap::parse(BUILTIN);
    assert_eq!(parsed.as_ref().err(), None);
    assert_eq!(parsed.map(|keymap| keymap.binds().len()), Ok(259));
}

#[test]
fn the_built_in_keymap_is_parsed_once() {
    assert!(std::ptr::eq(Keymap::builtin(), Keymap::builtin()));
    assert_eq!(Keymap::builtin().binds().len(), 259);
}

#[test]
fn a_bind_without_a_context_sits_in_the_home_context() {
    let text = one("tab.next", "\"ctrl-tab\"", None) + &one("jobs.toggle", "\"ctrl-j\"", None);
    let keymap = Keymap::parse(&text).expect("keymap");
    let placed: Vec<(&str, &str)> = keymap
        .binds()
        .iter()
        .map(|bind| (bind.spec.name, bind.context.as_str()))
        .collect();
    assert_eq!(
        placed,
        [("tab.next", "FerritePane"), ("jobs.toggle", "global")]
    );
}

#[test]
fn a_bind_may_sit_in_a_declared_descendant() {
    let text = one("viewer.back", "\"escape\"", Some("Tree"))
        + &one("context_menu.up", "\"up\"", Some("ContextMenu > Input"));
    assert_eq!(error_of(&text), None);
}

#[test]
fn keys_of_lists_every_key_of_a_command_in_file_order() {
    let text = one("files.rename", "\"f2\", \"shift-f6\"", None)
        + &one("viewer.back", "\"escape\"", Some("Tree"))
        + &one("viewer.back", "\"backspace\"", Some("Paged"));
    let keymap = Keymap::parse(&text).expect("keymap");
    let rename: Vec<&str> = keymap.keys_of("files.rename").collect();
    let back: Vec<&str> = keymap.keys_of("viewer.back").collect();
    assert_eq!(
        (rename, back),
        (vec!["f2", "shift-f6"], vec!["escape", "backspace"])
    );
}

#[test]
fn an_empty_key_list_is_allowed() {
    let keymap = Keymap::parse(&one("files.copy", "", None)).expect("keymap");
    assert_eq!(keymap.keys_of("files.copy").count(), 0);
}

#[test]
fn an_unknown_command_is_rejected() {
    assert_eq!(
        error_of(&one("files.teleport", "\"f5\"", None)),
        Some(KeymapError::UnknownCommand {
            command: "files.teleport".to_owned()
        })
    );
}

#[test]
fn a_bad_keystroke_is_rejected() {
    for key in ["ctrl-a-b", "", " "] {
        assert_eq!(
            error_of(&one("files.copy", &format!("\"{key}\""), None)),
            Some(KeymapError::BadKeystroke {
                command: "files.copy".to_owned(),
                key: key.to_owned()
            }),
            "{key:?}"
        );
    }
}

#[test]
fn a_context_outside_the_home_context_is_rejected() {
    let cases = [
        ("tab.next", "Tree"),
        ("viewer.back", "FileList"),
        ("jobs.toggle", "FileList"),
        ("files.copy", "global"),
    ];
    for (command, context) in cases {
        assert_eq!(
            error_of(&one(command, "\"f12\"", Some(context))),
            Some(KeymapError::ContextNotAllowed {
                command: command.to_owned(),
                context: context.to_owned()
            })
        );
    }
}

#[test]
fn a_key_twice_in_one_context_is_rejected() {
    let text = one("files.copy", "\"f5\"", None) + &one("files.move", "\"f5\"", None);
    assert_eq!(
        error_of(&text),
        Some(KeymapError::DuplicateKey {
            key: "f5".to_owned(),
            context: "FileList".to_owned()
        })
    );
}

#[test]
fn modifiers_in_another_order_are_the_same_key() {
    let text =
        one("files.copy", "\"ctrl-shift-x\"", None) + &one("files.move", "\"shift-ctrl-x\"", None);
    assert!(matches!(
        error_of(&text),
        Some(KeymapError::DuplicateKey { .. })
    ));
}

#[test]
fn the_same_key_in_two_contexts_is_allowed() {
    let text = one("files.copy", "\"f5\"", None) + &one("find.next", "\"f5\"", None);
    assert_eq!(error_of(&text), None);
}

#[test]
fn a_command_twice_in_one_context_is_rejected() {
    let text = one("files.copy", "\"f5\"", None) + &one("files.copy", "\"ctrl-c\"", None);
    assert_eq!(
        error_of(&text),
        Some(KeymapError::DuplicateEntry {
            command: "files.copy".to_owned(),
            context: "FileList".to_owned()
        })
    );
}

#[test]
fn broken_toml_and_unknown_fields_are_syntax_errors() {
    let unknown = "[[bind]]\ncommand = \"files.copy\"\nkeys = [\"f5\"]\nwhen = \"x\"\n";
    for text in ["[[bind]]\ncommand = 1\n", "[[bind]\n", unknown] {
        assert!(
            matches!(error_of(text), Some(KeymapError::Syntax(_))),
            "{text}"
        );
    }
}

#[test]
fn errors_name_what_is_wrong() {
    let error = KeymapError::ContextNotAllowed {
        command: "tab.next".to_owned(),
        context: "Tree".to_owned(),
    };
    let text = error.to_string();
    assert!(text.contains("tab.next") && text.contains("Tree"), "{text}");
}

#[test]
fn keys_are_shown_the_way_menus_write_them() {
    let keys = [
        "ctrl-k",
        "alt-f4",
        "f1",
        "ctrl-,",
        "-",
        "ctrl-shift-tab",
        "escape",
    ];
    let shown: Vec<Option<String>> = keys.into_iter().map(shown).collect();
    let expected = [
        "Ctrl+K",
        "Alt+F4",
        "F1",
        "Ctrl+,",
        "-",
        "Ctrl+Shift+Tab",
        "Escape",
    ];
    assert_eq!(shown, expected.map(|key| Some(key.to_owned())));
    assert_eq!(super::shown("ctrl-a-b"), None);
}
