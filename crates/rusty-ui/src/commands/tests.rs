use std::collections::HashSet;

use gpui_kit::{App, TestAppContext};
use rusty_l10n::{FILES, catalog, tr};

use super::{GLOBAL, Keymap, all, by_name, for_action, for_menu, is_within, key_bindings};
use crate::app_menu::MenuCommand;

const NAMESPACES: [&str; 23] = [
    "app_menu",
    "can_trace",
    "confirm_dialog",
    "context_menu",
    "dialog_focus",
    "drive_menu",
    "elf_view",
    "ferrite_tabs",
    "ferrite_window",
    "file_list",
    "map_view",
    "memory_image",
    "path_input",
    "preview",
    "prompt_dialog",
    "settings_sheet",
    "shortcuts",
    "side_sheet",
    "viewer",
    "viewer_markdown",
    "viewer_pdf",
    "viewer_table",
    "viewer_tree",
];

struct Bound {
    action: &'static str,
    context: String,
}

fn bound(cx: &mut TestAppContext) -> Vec<Bound> {
    cx.update(|cx: &mut App| {
        crate::window_view::bind_app_keys(cx);
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        keymap
            .bindings()
            .map(|binding| Bound {
                action: binding.action().name(),
                context: binding
                    .predicate()
                    .map_or_else(|| GLOBAL.to_owned(), |predicate| predicate.to_string()),
            })
            .collect()
    })
}

fn triples(cx: &mut TestAppContext, bind: impl FnOnce(&mut App)) -> Vec<String> {
    let mut lines: Vec<String> = cx.update(|cx: &mut App| {
        bind(cx);
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        keymap
            .bindings()
            .map(|binding| {
                let name = for_action(binding.action().name()).map_or("?", |spec| spec.name);
                let keys: Vec<String> = binding
                    .keystrokes()
                    .iter()
                    .map(|key| key.inner().unparse())
                    .collect();
                let context = binding
                    .predicate()
                    .map_or_else(|| GLOBAL.to_owned(), |predicate| predicate.to_string());
                format!("{name} | {} | {context}", keys.join(" "))
            })
            .collect()
    });
    lines.sort();
    lines.dedup();
    lines
}

fn ours(cx: &mut TestAppContext) -> Vec<&'static str> {
    cx.update(|cx: &mut App| {
        cx.all_action_names()
            .iter()
            .copied()
            .filter(|name| {
                name.split_once("::")
                    .is_some_and(|(space, _)| NAMESPACES.contains(&space))
            })
            .collect()
    })
}

fn is_word(part: &str) -> bool {
    part.split('_')
        .all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase()))
}

fn is_area_verb(name: &str) -> bool {
    name.split_once('.')
        .is_some_and(|(area, verb)| is_word(area) && is_word(verb))
}

#[gpui_kit::test]
fn every_bound_action_has_exactly_one_command(cx: &mut TestAppContext) {
    let missing: Vec<&str> = bound(cx)
        .iter()
        .map(|binding| binding.action)
        .filter(|&action| all().filter(|spec| spec.action == Some(action)).count() != 1)
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
}

#[gpui_kit::test]
fn every_ferrite_action_has_a_command(cx: &mut TestAppContext) {
    let missing: Vec<&str> = ours(cx)
        .into_iter()
        .filter(|action| for_action(action).is_none())
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
}

#[gpui_kit::test]
fn every_command_action_exists(cx: &mut TestAppContext) {
    let known = ours(cx);
    let unknown: Vec<&str> = all()
        .filter_map(|spec| spec.action)
        .filter(|action| !known.contains(action))
        .collect();
    assert_eq!(unknown, Vec::<&str>::new());
}

#[gpui_kit::test]
fn bindings_sit_in_their_command_context(cx: &mut TestAppContext) {
    let wrong: Vec<String> = bound(cx)
        .iter()
        .filter_map(|binding| {
            let spec = for_action(binding.action)?;
            let fits = if spec.context == GLOBAL {
                binding.context == GLOBAL
            } else {
                is_within(&binding.context, spec.context)
            };
            (!fits).then(|| format!("{} in {}", spec.name, binding.context))
        })
        .collect();
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn names_are_unique() {
    let mut seen = HashSet::new();
    let doubled: Vec<&str> = all()
        .map(|spec| spec.name)
        .filter(|name| !seen.insert(*name))
        .collect();
    assert_eq!(doubled, Vec::<&str>::new());
}

#[test]
fn names_are_area_dot_verb_in_snake_case() {
    let bad: Vec<&str> = all()
        .map(|spec| spec.name)
        .filter(|name| !is_area_verb(name))
        .collect();
    assert_eq!(bad, Vec::<&str>::new());
}

#[test]
fn the_name_check_rejects_other_shapes() {
    let shapes = [
        "tree.expand_all",
        "tree",
        "Tree.open",
        "pane.open-tab",
        "tab.go_to_1",
    ];
    let accepted: Vec<bool> = shapes.iter().map(|name| is_area_verb(name)).collect();
    assert_eq!(accepted, [true, false, false, false, false]);
}

#[test]
fn every_command_has_a_label_and_a_context() {
    let empty: Vec<&str> = all()
        .filter(|spec| tr(spec.label).trim().is_empty() || spec.context.trim().is_empty())
        .map(|spec| spec.name)
        .collect();
    assert_eq!(empty, Vec::<&str>::new());
}

#[test]
fn nested_contexts_fall_inside_their_parent() {
    assert!(is_within("Tree", "Viewer"));
    assert!(is_within("ContextMenu > Input", "ContextMenu"));
    assert!(is_within("ViewerFind > Input", "Viewer"));
    assert!(is_within("DialogButton", "Dialog"));
    assert!(!is_within("PreviewBrowse", "FerritePane"));
    assert!(!is_within("Viewer", "Tree"));
}

#[test]
fn every_menu_row_names_a_registered_command() {
    let rows = [
        MenuCommand::CommandPalette,
        MenuCommand::NewWindow,
        MenuCommand::Workspaces,
        MenuCommand::Help,
        MenuCommand::Quit,
        MenuCommand::AllSettings,
    ];
    let names: Vec<Option<&str>> = rows
        .into_iter()
        .map(|row| by_name(for_menu(row)).map(|spec| spec.name))
        .collect();
    assert_eq!(
        names,
        [
            Some("app.command_palette"),
            Some("window.new"),
            Some("workspace.list"),
            Some("help.key_map"),
            Some("app.quit"),
            Some("settings.toggle"),
        ]
    );
}

#[test]
fn all_settings_shares_the_toggle_settings_command() {
    let toggle = for_action("settings_sheet::ToggleSettings").map(|spec| spec.name);
    assert_eq!(toggle, Some(for_menu(MenuCommand::AllSettings)));
}

#[test]
fn no_command_toggles_the_preview() {
    let names: Vec<&str> = all().map(|spec| spec.name).collect();
    assert!(!names.contains(&"pane.toggle_preview"));
    assert!(!names.contains(&"preview.toggle"));
}

#[test]
fn the_command_list_matches_the_snapshot() {
    let lines: Vec<String> = all()
        .map(|spec| format!("{} | {} | {}", spec.name, tr(spec.label), spec.context))
        .collect();
    insta::with_settings!({ prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("command_registry", lines.join("\n"));
    });
}

#[gpui_kit::test]
fn the_bound_keys_match_the_snapshot(cx: &mut TestAppContext) {
    let lines = triples(cx, crate::window_view::bind_app_keys);
    insta::with_settings!({ prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("bound_keys", lines.join("\n"));
    });
}

fn in_bound_order(cx: &mut TestAppContext) -> Vec<String> {
    let bound: Vec<(String, String)> = cx.update(|cx: &mut App| {
        crate::window_view::bind_app_keys(cx);
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        keymap
            .bindings()
            .map(|binding| {
                let action = binding.action().name();
                let space = action.split_once("::").map_or(action, |(space, _)| space);
                let name = for_action(action).map_or("?", |spec| spec.name);
                let keys: Vec<String> = binding
                    .keystrokes()
                    .iter()
                    .map(|key| key.inner().unparse())
                    .collect();
                let context = binding
                    .predicate()
                    .map_or_else(|| GLOBAL.to_owned(), |predicate| predicate.to_string());
                (
                    space.to_owned(),
                    format!("{name} | {} | {context}", keys.join(" ")),
                )
            })
            .collect()
    });
    let mut spaces: Vec<&str> = Vec::new();
    for (space, _) in &bound {
        if !spaces.contains(&space.as_str()) {
            spaces.push(space);
        }
    }
    let mut lines = Vec::new();
    for space in spaces {
        lines.push(format!("[{space}]"));
        lines.extend(
            bound
                .iter()
                .filter(|(of, _)| of == space)
                .map(|(_, line)| line.clone()),
        );
    }
    lines
}

#[gpui_kit::test]
fn the_bound_keys_keep_their_order(cx: &mut TestAppContext) {
    let lines = in_bound_order(cx);
    insta::with_settings!({ prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("bound_order", lines.join("\n"));
    });
}

#[gpui_kit::test]
fn the_built_in_keymap_builds_the_snapshot_set(cx: &mut TestAppContext) {
    let lines = triples(cx, |cx| {
        let bindings = key_bindings(Keymap::builtin(), cx, None);
        cx.bind_keys(bindings);
    });
    insta::with_settings!({ prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("bound_keys", lines.join("\n"));
    });
}

#[gpui_kit::test]
fn a_module_binds_only_its_own_commands(cx: &mut TestAppContext) {
    let actions: Vec<&'static str> = cx.update(|cx: &mut App| {
        crate::file_list::bind_keys(cx);
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        keymap
            .bindings()
            .map(|binding| binding.action().name())
            .collect()
    });
    let foreign: Vec<&str> = actions
        .iter()
        .copied()
        .filter(|action| !action.starts_with("file_list::"))
        .collect();
    assert_eq!((actions.len(), foreign), (40, Vec::<&str>::new()));
}

#[gpui_kit::test]
fn every_keymap_command_with_an_action_builds(cx: &mut TestAppContext) {
    let broken: Vec<&str> = cx.update(|cx: &mut App| {
        Keymap::builtin()
            .binds()
            .iter()
            .filter_map(|bind| bind.spec.action)
            .filter(|action| cx.build_action(action, None).is_err())
            .collect()
    });
    assert_eq!(broken, Vec::<&str>::new());
}

#[test]
fn pdf_keys_shadow_only_the_viewer_marks_and_escape() {
    let binds = Keymap::builtin().binds();
    let in_context = |context: &str| -> Vec<(String, &'static str)> {
        binds
            .iter()
            .filter(|bind| bind.context == context)
            .flat_map(|bind| {
                bind.keys
                    .iter()
                    .map(|key| (super::normal_key(key).unwrap_or_default(), bind.spec.name))
            })
            .collect()
    };
    let viewer = in_context("Viewer");
    let shadowed: Vec<(String, &str, &str)> = in_context("Pdf")
        .into_iter()
        .filter_map(|(key, pdf)| {
            let (_, under) = viewer.iter().find(|(used, _)| *used == key)?;
            Some((key, *under, pdf))
        })
        .collect();
    assert_eq!(
        shadowed,
        [
            ("escape".to_owned(), "viewer.close", "viewer.back"),
            ("ctrl-0".to_owned(), "mark.clear", "pdf.fit"),
            ("ctrl-1".to_owned(), "mark.colour_one", "pdf.actual_size"),
        ]
    );
}

#[test]
fn drawer_keys_belong_to_the_shared_view_commands_in_every_viewer() {
    let binds = Keymap::builtin().binds();
    let keyed = |key: &str| -> Vec<(String, &str)> {
        binds
            .iter()
            .filter(|bind| bind.keys.iter().any(|used| used == key))
            .map(|bind| (bind.context.clone(), bind.spec.name))
            .collect()
    };
    assert_eq!(keyed("ctrl-b"), [("Viewer".to_owned(), "view.left_drawer")]);
    assert_eq!(
        keyed("ctrl-i"),
        [("Viewer".to_owned(), "view.right_drawer")]
    );
}

#[test]
fn every_command_label_is_the_cmd_id_of_its_name() {
    let wrong: Vec<&str> = all()
        .filter(|spec| spec.label != format!("cmd-{}", spec.name.replace(['.', '_'], "-")))
        .map(|spec| spec.name)
        .collect();
    assert_eq!(wrong, Vec::<&str>::new());
}

#[test]
fn every_command_label_exists_in_the_bundle() {
    let missing: Vec<&str> = all()
        .filter(|spec| !catalog().has(spec.label))
        .map(|spec| spec.label)
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn every_cmd_message_in_the_files_belongs_to_a_command() {
    let labels: HashSet<&str> = all().map(|spec| spec.label).collect();
    let orphans: Vec<&str> = FILES
        .iter()
        .flat_map(|(_, source)| source.lines())
        .filter_map(|line| line.split(" =").next())
        .filter(|id| id.starts_with("cmd-") && !labels.contains(id))
        .collect();
    assert_eq!(orphans, Vec::<&str>::new());
}
