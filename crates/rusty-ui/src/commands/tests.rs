use std::collections::HashSet;

use gpui_kit::{App, TestAppContext};

use super::{CommandSpec, GLOBAL, all, by_name, for_action, for_menu, is_within};
use crate::app_menu::MenuCommand;

const NAMESPACES: [&str; 17] = [
    "app_menu",
    "confirm_dialog",
    "context_menu",
    "dialog_focus",
    "drive_menu",
    "ferrite_tabs",
    "ferrite_window",
    "file_list",
    "path_input",
    "preview",
    "prompt_dialog",
    "settings_sheet",
    "side_sheet",
    "viewer",
    "viewer_markdown",
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
        .filter(|spec| spec.label.trim().is_empty() || spec.context.trim().is_empty())
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
fn the_preview_toggles_are_two_commands() {
    let pane = for_action("ferrite_window::TogglePreview");
    let browse = for_action("preview::TogglePreview");
    let pick = |spec: Option<CommandSpec>| spec.map(|spec| (spec.name, spec.context));
    assert_eq!(pick(pane), Some(("pane.toggle_preview", "FerritePane")));
    assert_eq!(pick(browse), Some(("preview.toggle", "PreviewBrowse")));
}

#[test]
fn the_command_list_matches_the_snapshot() {
    let lines: Vec<String> = all()
        .map(|spec| format!("{} | {} | {}", spec.name, spec.label, spec.context))
        .collect();
    insta::with_settings!({ prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("command_registry", lines.join("\n"));
    });
}
