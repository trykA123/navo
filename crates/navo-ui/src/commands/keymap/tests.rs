use super::{BUILTIN, Keymap, KeymapError, KeymapWarning, shown};

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
    assert_eq!(parsed.map(|keymap| keymap.binds().len()), Ok(547));
}

#[test]
fn the_built_in_keymap_is_parsed_once() {
    assert!(std::ptr::eq(Keymap::builtin(), Keymap::builtin()));
    assert_eq!(Keymap::builtin().binds().len(), 547);
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
        [("tab.next", "NavoPane"), ("jobs.toggle", "global")]
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

fn placed(name: &str) -> Vec<(String, Vec<String>)> {
    Keymap::builtin()
        .binds()
        .iter()
        .filter(|bind| bind.spec.name == name)
        .map(|bind| (bind.context.clone(), bind.keys.clone()))
        .collect()
}

#[test]
fn the_view_menu_and_drawer_keys_sit_in_the_viewer_context() {
    let viewer = |key: &str| vec![("Viewer".to_owned(), vec![key.to_owned()])];
    assert_eq!(placed("viewer.view_menu"), viewer("alt-v"));
    assert_eq!(placed("view.left_drawer"), viewer("ctrl-b"));
    assert_eq!(placed("view.right_drawer"), viewer("ctrl-i"));
}

#[test]
fn escape_is_one_back_key_for_the_viewer_and_memory_drawers_use_the_shared_keys() {
    assert_eq!(
        placed("viewer.back"),
        [("Viewer".to_owned(), vec!["escape".to_owned()])]
    );
    assert!(placed("viewer.close").is_empty());
    assert!(placed("memory.toggle_navigator").is_empty());
    assert!(placed("memory.toggle_inspector").is_empty());
    assert_eq!(
        placed("image.toggle_info"),
        [("Image".to_owned(), vec!["i".to_owned()])]
    );
}

#[test]
fn an_old_pane_context_binds_as_the_new_one() {
    let text = one("tab.next", "\"ctrl-tab\"", Some("FerritePane"));
    let keymap = Keymap::parse(&text).expect("keymap");
    let contexts: Vec<&str> = keymap.binds().iter().map(|b| b.context.as_str()).collect();
    assert_eq!(contexts, ["NavoPane"]);
}

#[test]
fn an_unknown_context_is_still_refused() {
    let text = one("tab.next", "\"ctrl-tab\"", Some("FerriteOther"));
    assert!(matches!(
        error_of(&text),
        Some(KeymapError::ContextNotAllowed { .. })
    ));
}

fn compare_binds(keymap: &Keymap) -> Vec<(&'static str, String, Vec<String>)> {
    keymap
        .binds()
        .iter()
        .filter(|bind| bind.spec.name.starts_with("compare."))
        .map(|bind| (bind.spec.name, bind.context.clone(), bind.keys.clone()))
        .collect()
}

#[test]
fn hidden_compare_binds_are_left_out_of_the_built_in_and_user_keymaps() {
    let _hidden = crate::compare_visible::hide_compare();
    let builtin = Keymap::parse(BUILTIN).expect("keymap");
    assert_eq!(compare_binds(&builtin), []);
    let user = one("compare.exit", "\"escape\"", None) + &one("compare.nope", "\"f1\"", None);
    let overlaid = builtin.overlay(&user).expect("overlay");
    assert_eq!(compare_binds(&overlaid.keymap), []);
}

#[test]
fn visible_compare_binds_follow_the_key_table() {
    let _shown = crate::compare_visible::show_compare();
    let keymap = Keymap::parse(BUILTIN).expect("keymap");
    let keyed = |name: &str, context: &str| -> Vec<String> {
        compare_binds(&keymap)
            .into_iter()
            .filter(|(bound, at, _)| *bound == name && at == context)
            .flat_map(|(_, _, keys)| keys)
            .collect()
    };
    assert_eq!(compare_binds(&keymap).len(), 33);
    assert_eq!(
        keyed("compare.with_other_pane", "FileList"),
        ["ctrl-shift-d"]
    );
    assert_eq!(keyed("compare.next_difference", "Compare"), ["alt-down"]);
    assert_eq!(
        keyed("compare.next_difference", "CompareView"),
        ["alt-down", "f7"]
    );
    assert_eq!(
        keyed("compare.previous_difference", "CompareView"),
        ["alt-up", "shift-f7"]
    );
    assert_eq!(keyed("compare.enter", "Compare"), ["enter"]);
    assert_eq!(keyed("compare.up", "Compare"), ["backspace"]);
    assert_eq!(keyed("compare.continue", "Compare"), ["ctrl-f7"]);
    assert_eq!(keyed("compare.exit", "CompareView"), ["escape"]);
    assert_eq!(keyed("compare.save", "CompareView"), Vec::<String>::new());
    assert_eq!(keyed("compare.copy_right", "Compare"), Vec::<String>::new());
    assert_eq!(keyed("compare.copy_left", "Compare"), Vec::<String>::new());
    assert_eq!(
        keyed("compare.copy_right", "CompareView"),
        ["ctrl-alt-right"]
    );
    assert_eq!(keyed("compare.copy_left", "CompareView"), ["ctrl-alt-left"]);
    assert_eq!(keyed("compare.enter", "CompareView"), Vec::<String>::new());
    assert_eq!(keyed("compare.stream", "Compare"), ["alt-f"]);
    assert_eq!(keyed("compare.next_file", "CompareView"), ["ctrl-alt-down"]);
    assert_eq!(
        keyed("compare.next_file", "CompareStream"),
        ["shift-j", "ctrl-alt-down"]
    );
    assert_eq!(keyed("compare.open_file", "CompareStream"), ["enter"]);
    assert_eq!(keyed("compare.toggle_viewed", "CompareStream"), ["v"]);
    assert_eq!(keyed("compare.fold", "CompareStream"), ["o"]);
    assert_eq!(keyed("compare.view_menu", "CompareView"), ["alt-v"]);
    assert_eq!(keyed("compare.changes", "CompareView"), ["ctrl-b"]);
    assert_eq!(keyed("compare.details", "CompareView"), ["ctrl-i"]);
}

#[test]
fn visible_pick_binds_follow_the_key_table() {
    let _shown = crate::compare_visible::show_compare();
    let keymap = Keymap::parse(BUILTIN).expect("keymap");
    let keyed = |name: &str, context: &str| -> Vec<String> {
        compare_binds(&keymap)
            .into_iter()
            .filter(|(bound, at, _)| *bound == name && at == context)
            .flat_map(|(_, _, keys)| keys)
            .collect()
    };
    assert_eq!(keyed("compare.pick_down", "CompareView"), ["shift-down"]);
    assert_eq!(keyed("compare.pick_up", "CompareView"), ["shift-up"]);
    assert_eq!(keyed("compare.pick_toggle", "CompareView"), ["space"]);
    assert_eq!(
        keyed("compare.remove_picked", "CompareView"),
        ["ctrl-shift-delete"]
    );
}

#[gpui_kit::test]
fn visible_compare_binds_build_gpui_key_bindings(cx: &mut gpui_kit::TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let keymap = Keymap::parse(BUILTIN).expect("keymap");
    let built = cx.update(|cx| crate::commands::key_bindings(&keymap, cx, Some("compare")));
    assert_eq!(built.len(), 38);
}

mod lookups {
    use gpui_kit::Keystroke;

    use super::{Keymap, one};

    fn keymap() -> Keymap {
        let text = one("files.open", "\"enter\"", Some("FileList"))
            + &one("viewer.back", "\"escape\"", Some("Viewer"))
            + &one("files.open_parent", "\"ctrl-shift-3\"", Some("FileList"));
        Keymap::parse(&text).expect("keymap")
    }

    fn stroke(text: &str) -> Keystroke {
        Keystroke::parse(text).expect("stroke")
    }

    #[test]
    fn a_key_is_found_only_in_its_own_context() {
        let keymap = keymap();
        assert_eq!(
            keymap.keys_in("viewer.back", "Viewer").collect::<Vec<_>>(),
            ["escape"]
        );
        assert_eq!(keymap.keys_in("viewer.back", "FileList").count(), 0);
        assert!(!keymap.has_key_in("viewer.back", "FileList", &stroke("escape")));
    }

    #[test]
    fn a_user_override_replaces_the_built_in_key() {
        let overlaid = keymap()
            .overlay(&one("viewer.back", "\"ctrl-b\"", Some("Viewer")))
            .expect("overlay");
        let keys: Vec<_> = overlaid.keymap.keys_in("viewer.back", "Viewer").collect();
        assert_eq!(keys, ["ctrl-b"]);
        assert_eq!(
            overlaid.keymap.shown_in("viewer.back", "Viewer").as_deref(),
            Some("Ctrl+B")
        );
    }

    #[test]
    fn has_key_in_matches_modifiers() {
        let keymap = keymap();
        assert!(keymap.has_key_in("files.open", "FileList", &stroke("enter")));
        assert!(!keymap.has_key_in("files.open", "FileList", &stroke("ctrl-enter")));
        assert!(keymap.has_key_in("files.open_parent", "FileList", &stroke("ctrl-shift-3")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn has_key_in_matches_a_shifted_digit_symbol_on_linux() {
        assert!(keymap().has_key_in("files.open_parent", "FileList", &stroke("ctrl-#")));
    }

    #[test]
    fn shown_in_of_an_unbound_command_is_none() {
        assert_eq!(keymap().shown_in("files.open", "Viewer"), None);
        assert_eq!(keymap().shown_in("nope", "FileList"), None);
    }
}

#[test]
fn a_bare_digit_for_a_layout_verb_warns_and_stays_in_the_keymap() {
    let text = one("layout_mode.split_right", "\"1\", \"alt-1\"", None);
    let result = Keymap::builtin().overlay(&text).expect("keymap");
    let keys: Vec<_> = result
        .keymap
        .keys_in("layout_mode.split_right", "LayoutMode")
        .collect();
    assert_eq!(keys, ["1", "alt-1"]);
    let command = "layout_mode.split_right".to_owned();
    let fixed = KeymapWarning::Fixed {
        line: 1,
        key: "1".to_owned(),
        command,
    };
    assert_eq!(result.warnings, [fixed]);
}

#[test]
fn visible_compare_menu_opens_with_shift_f10_and_the_menu_key() {
    let _shown = crate::compare_visible::show_compare();
    let keymap = Keymap::parse(BUILTIN).expect("keymap");
    let keys: Vec<String> = compare_binds(&keymap)
        .into_iter()
        .filter(|(bound, at, _)| *bound == "compare.show_menu" && at == "CompareView")
        .flat_map(|(_, _, keys)| keys)
        .collect();
    assert_eq!(keys, ["menu", "shift-f10"]);
}
