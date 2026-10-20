use gpui_kit::Keystroke;

use super::*;
use crate::commands::{Keymap, all, by_name};

fn spec(name: &str) -> crate::commands::CommandSpec {
    by_name(name).expect("a registered command")
}

fn row<'a>(rows: &'a [Row], name: &str) -> &'a Row {
    rows.iter()
        .find(|row| row.spec.name == name)
        .expect("a row for every command")
}

fn shown(rows: &[Row], query: &str) -> Vec<&'static str> {
    lines(rows, query)
        .into_iter()
        .filter_map(|line| match line {
            Line::Row(index) => rows.get(index).map(|row| row.spec.name),
            Line::Area(_) => None,
        })
        .collect()
}

fn user(text: &str) -> Keymap {
    Keymap::builtin()
        .overlay(text)
        .expect("valid user keymap")
        .keymap
}

fn proposal(name: &str, keys: &[&str]) -> Proposal {
    Proposal {
        spec: spec(name),
        keys: keys.iter().map(|&key| key.to_owned()).collect(),
    }
}

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|&key| key.to_owned()).collect()
}

fn stroke(text: &str) -> Keystroke {
    Keystroke::parse(text).expect("a keystroke")
}

#[test]
fn there_is_one_row_per_command_in_registry_order() {
    let rows = rows(Keymap::builtin());
    let names: Vec<&str> = rows.iter().map(|row| row.spec.name).collect();
    let registry: Vec<&str> = all().map(|spec| spec.name).collect();
    assert_eq!(names, registry);
    assert_eq!(row(&rows, "files.sort_by_date").keys, ["ctrl-f5"]);
    assert!(!row(&rows, "files.sort_by_date").is_changed);
}

#[test]
fn a_key_bound_in_several_contexts_shows_once() {
    let rows = rows(Keymap::builtin());
    assert_eq!(row(&rows, "viewer.back").keys, ["escape"]);
    assert_eq!(row(&rows, "viewer.go_to_start").keys, ["ctrl-home", "home"]);
}

#[test]
fn a_menu_only_command_has_a_row_with_its_shown_key() {
    let rows = rows(Keymap::builtin());
    assert_eq!(row(&rows, "app.quit").keys, ["alt-f4"]);
}

#[test]
fn a_user_entry_marks_its_row_changed() {
    let keymap = user("[[bind]]\ncommand = \"files.copy\"\nkeys = [\"alt-f5\"]\n");
    let rows = rows(&keymap);
    assert!(row(&rows, "files.copy").is_changed);
    assert_eq!(row(&rows, "files.copy").keys, ["alt-f5"]);
    assert!(!row(&rows, "files.move").is_changed);
}

#[test]
fn lines_group_rows_by_area_in_order_of_first_use() {
    let rows = rows(Keymap::builtin());
    let lines = lines(&rows, "");
    let areas: Vec<&str> = lines
        .iter()
        .filter_map(|line| match line {
            Line::Area(area) => Some(*area),
            Line::Row(_) => None,
        })
        .collect();
    assert_eq!(areas.first(), Some(&"app_menu"));
    assert_eq!(areas.iter().filter(|area| **area == "pane").count(), 1);
    assert_eq!(areas.iter().filter(|area| **area == "files").count(), 1);
    let at = |area: &str| areas.iter().position(|shown| *shown == area);
    assert!(at("tab") < at("pane") && at("pane") < at("files"));
    assert_eq!(lines.len(), rows.len() + areas.len());
    let mut current = "";
    for line in &lines {
        match line {
            Line::Area(area) => current = area,
            Line::Row(index) => assert!(rows[*index].spec.name.starts_with(&format!("{current}."))),
        }
    }
}

#[test]
fn an_empty_search_shows_every_row() {
    let rows = rows(Keymap::builtin());
    assert_eq!(shown(&rows, "").len(), rows.len());
    assert_eq!(shown(&rows, "   ").len(), rows.len());
}

#[test]
fn search_finds_a_label_in_any_case() {
    let rows = rows(Keymap::builtin());
    assert_eq!(shown(&rows, "sort by size"), ["files.sort_by_size"]);
    assert_eq!(shown(&rows, "SORT BY SIZE"), ["files.sort_by_size"]);
    let lines = lines(&rows, "sort by size");
    assert_eq!(lines.first(), Some(&Line::Area("files")));
}

#[test]
fn search_finds_a_command_name() {
    let rows = rows(Keymap::builtin());
    assert_eq!(shown(&rows, "files.sort_by_date"), ["files.sort_by_date"]);
}

const CTRL_F5: [&str; 3] = [
    "files.sort_by_date",
    "elf.sort_by_address",
    "map.sort_by_address",
];

#[test]
fn search_finds_the_command_bound_to_a_key() {
    let rows = rows(Keymap::builtin());
    assert_eq!(shown(&rows, "ctrl+f5"), CTRL_F5);
    assert_eq!(shown(&rows, "Ctrl+F5"), CTRL_F5);
    assert_eq!(shown(&rows, "ctrl-f5"), CTRL_F5);
}

#[test]
fn a_key_search_takes_modifiers_in_any_order_and_spaces() {
    let rows = rows(Keymap::builtin());
    for typed in ["f5 ctrl", "F5+CTRL", "ctrl - f5", "ctrl f5", " f5-ctrl "] {
        assert_eq!(shown(&rows, typed), CTRL_F5, "{typed}");
    }
    let tab = ["tab.previous"];
    assert_eq!(shown(&rows, "shift+ctrl+tab"), tab);
    assert_eq!(shown(&rows, "tab shift ctrl"), tab);
}

#[test]
fn areas_read_as_words() {
    assert_eq!(area_label("app_menu"), "App menu");
    assert_eq!(area_label("files"), "Files");
}

#[test]
fn a_key_used_in_the_same_context_is_a_conflict() {
    let found = conflict(
        Keymap::builtin(),
        &proposal("files.sort_by_size", &["ctrl-f5"]),
    );
    let expected = Conflict {
        key: "ctrl-f5".to_owned(),
        other: spec("files.sort_by_date"),
        context: "FileList".to_owned(),
        reach: Reach::Same,
    };
    assert_eq!(found, Some(expected));
    assert_eq!(
        found.map(|found| found.message()).as_deref(),
        Some("Ctrl+F5 is used by Sort by date — replace it?")
    );
}

#[test]
fn a_key_of_another_context_or_of_the_command_itself_is_free() {
    let keymap = Keymap::builtin();
    assert_eq!(
        conflict(keymap, &proposal("files.sort_by_size", &["ctrl-g"])),
        None
    );
    assert_eq!(
        conflict(keymap, &proposal("files.sort_by_date", &["ctrl-f5"])),
        None
    );
    assert_eq!(conflict(keymap, &proposal("files.sort_by_size", &[])), None);
}

fn set_in(command: &'static str, context: Option<&str>, list: &[&str]) -> Edit {
    Edit::Set {
        command,
        context: context.map(str::to_owned),
        keys: keys(list),
    }
}

const BACK_CONTEXTS: [&str; 10] = [
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
];

#[test]
fn a_command_bound_in_several_contexts_is_recorded_in_each() {
    let edits = edits(Keymap::builtin(), &proposal("viewer.back", &["f8"]));
    let expected: Vec<Edit> = BACK_CONTEXTS
        .iter()
        .map(|&context| set_in("viewer.back", Some(context), &["f8"]))
        .collect();
    assert_eq!(edits, expected);
}

#[test]
fn an_unbind_clears_every_context_of_the_command() {
    let edits = edits(Keymap::builtin(), &proposal("viewer.back", &[]));
    let expected: Vec<Edit> = BACK_CONTEXTS
        .iter()
        .map(|&context| set_in("viewer.back", Some(context), &[]))
        .collect();
    assert_eq!(edits, expected);
}

#[test]
fn the_home_context_is_written_without_a_context() {
    let edits = edits(Keymap::builtin(), &proposal("viewer.go_to_start", &["f8"]));
    let expected = [
        set_in("viewer.go_to_start", None, &["f8"]),
        set_in("viewer.go_to_start", Some("Archive"), &["f8"]),
        set_in("viewer.go_to_start", Some("Tree"), &["f8"]),
        set_in("viewer.go_to_start", Some("Pdf"), &["f8"]),
    ];
    assert_eq!(edits, expected);
}

#[test]
fn a_command_bound_nowhere_is_set_in_its_home_context() {
    let keymap = Keymap::parse("").expect("an empty keymap");
    let edits = edits(&keymap, &proposal("viewer.back", &["f8"]));
    assert_eq!(edits, [set_in("viewer.back", None, &["f8"])]);
}

#[test]
fn a_key_of_any_context_the_command_is_bound_in_is_a_conflict() {
    let found = conflict(Keymap::builtin(), &proposal("viewer.back", &["alt-left"]));
    let expected = Conflict {
        key: "alt-left".to_owned(),
        other: spec("pdf.trail_back"),
        context: "Pdf".to_owned(),
        reach: Reach::Same,
    };
    assert_eq!(found, Some(expected));
}

#[test]
fn a_key_of_an_enclosing_context_is_a_conflict_the_new_key_would_hide() {
    let found = conflict(Keymap::builtin(), &proposal("files.sort_by_size", &["f10"]));
    let expected = Conflict {
        key: "f10".to_owned(),
        other: spec("app_menu.toggle"),
        context: "global".to_owned(),
        reach: Reach::Outer,
    };
    assert_eq!(found.as_ref(), Some(&expected));
    assert_eq!(
        expected.message(),
        "F10 is used by App menu — your key would hide it here. Replace?"
    );
    let found = conflict(
        Keymap::builtin(),
        &proposal("settings.toggle", &["ctrl-f5"]),
    );
    let reach = found.map(|found| (found.other.name, found.reach));
    assert_eq!(reach, Some(("files.sort_by_date", Reach::Outer)));
}

#[test]
fn a_key_of_a_nested_context_is_a_conflict_that_would_hide_the_new_key() {
    let found = conflict(
        Keymap::builtin(),
        &proposal("lister.show_text", &["alt-left"]),
    );
    let expected = Conflict {
        key: "alt-left".to_owned(),
        other: spec("pdf.trail_back"),
        context: "Pdf".to_owned(),
        reach: Reach::Inner,
    };
    assert_eq!(found.as_ref(), Some(&expected));
    assert_eq!(
        expected.message(),
        "Alt+Left is used by Back — it would hide your key there. Replace?"
    );
}

#[test]
fn replacing_a_key_of_a_nested_context_takes_it_from_there() {
    let edits = edits(
        Keymap::builtin(),
        &proposal("lister.show_text", &["alt-left"]),
    );
    let expected = [
        set_in("lister.show_text", None, &["alt-left"]),
        set_in("pdf.trail_back", None, &[]),
        set_in("tree.collapse_branch", None, &[]),
    ];
    assert_eq!(edits, expected);
}

#[test]
fn replacing_a_key_of_an_enclosing_context_leaves_it_there() {
    let edits = edits(Keymap::builtin(), &proposal("files.sort_by_size", &["f10"]));
    assert_eq!(edits, [set_in("files.sort_by_size", None, &["f10"])]);
    let global = proposal("settings.toggle", &["ctrl-f5"]);
    assert_eq!(
        super::edits(Keymap::builtin(), &global),
        [set_in("settings.toggle", None, &["ctrl-f5"])]
    );
}

#[test]
fn a_user_entry_with_the_default_keys_is_no_change() {
    let keymap = user("[[bind]]\ncommand = \"files.copy\"\nkeys = [\"f5\"]\n");
    assert!(!row(&rows(&keymap), "files.copy").is_changed);
    let keymap =
        user("[[bind]]\ncommand = \"files.copy\"\nkeys = [\"f5\"]\ncontext = \"FileList\"\n");
    assert!(!row(&rows(&keymap), "files.copy").is_changed);
}

#[test]
fn replacing_a_built_in_key_writes_only_the_new_binding() {
    let edits = edits(
        Keymap::builtin(),
        &proposal("files.sort_by_size", &["ctrl-f5"]),
    );
    let expected = Edit::Set {
        command: "files.sort_by_size",
        context: None,
        keys: keys(&["ctrl-f5"]),
    };
    assert_eq!(edits, [expected]);
}

#[test]
fn replacing_a_key_of_another_user_entry_takes_it_from_that_entry() {
    let keymap =
        user("[[bind]]\ncommand = \"files.sort_by_name\"\nkeys = [\"alt-n\", \"ctrl-f9\"]\n");
    let edits = edits(&keymap, &proposal("files.sort_by_size", &["ctrl-f9"]));
    let mine = Edit::Set {
        command: "files.sort_by_size",
        context: None,
        keys: keys(&["ctrl-f9"]),
    };
    let theirs = Edit::Set {
        command: "files.sort_by_name",
        context: None,
        keys: keys(&["alt-n"]),
    };
    assert_eq!(edits, [mine, theirs]);
}

#[test]
fn an_unbind_writes_an_empty_key_list() {
    let edits = edits(Keymap::builtin(), &proposal("files.copy", &[]));
    let expected = Edit::Set {
        command: "files.copy",
        context: None,
        keys: Vec::new(),
    };
    assert_eq!(edits, [expected]);
}

#[test]
fn a_chord_is_recorded_in_keymap_form() {
    assert_eq!(
        recorded(&stroke("ctrl-f5")),
        Some(Recorded::Key("ctrl-f5".to_owned()))
    );
    assert_eq!(
        recorded(&stroke("shift-a")),
        Some(Recorded::Key("shift-a".to_owned()))
    );
    assert_eq!(
        recorded(&stroke("ctrl-escape")),
        Some(Recorded::Key("ctrl-escape".to_owned()))
    );
}

#[test]
fn escape_cancels_and_backspace_unbinds() {
    assert_eq!(recorded(&stroke("escape")), Some(Recorded::Cancel));
    assert_eq!(recorded(&stroke("backspace")), Some(Recorded::Unbind));
}

#[test]
fn a_modifier_alone_is_not_a_key() {
    assert_eq!(recorded(&stroke("shift")), None);
    assert_eq!(recorded(&stroke("ctrl-control")), None);
    assert_eq!(recorded(&stroke("alt")), None);
}
