use gpui_kit::{App, TestAppContext};
use navo_l10n::catalog;

use super::{all, by_name, entries, for_action, registry};
use crate::compare_visible::hide_compare;

fn compare_names() -> Vec<&'static str> {
    registry()
        .map(|spec| spec.name)
        .filter(|name| name.starts_with("compare."))
        .collect()
}

#[test]
fn the_compare_commands_are_registered_in_the_planned_contexts() {
    let placed: Vec<(&str, &str)> = registry()
        .filter(|spec| spec.name.starts_with("compare."))
        .map(|spec| (spec.name, spec.context))
        .collect();
    assert_eq!(
        placed,
        [
            ("compare.with_other_pane", "FileList"),
            ("compare.refs", "global"),
            ("compare.next_difference", "Compare"),
            ("compare.previous_difference", "Compare"),
            ("compare.copy_right", "Compare"),
            ("compare.copy_left", "Compare"),
            ("compare.enter", "Compare"),
            ("compare.up", "Compare"),
            ("compare.exit", "Compare"),
            ("compare.stop_checking", "Compare"),
            ("compare.continue", "Compare"),
            ("compare.stream", "Compare"),
            ("compare.save", "CompareView"),
            ("compare.view_bytes", "CompareView"),
            ("compare.view_text", "CompareView"),
            ("compare.next_file", "CompareView"),
            ("compare.previous_file", "CompareView"),
            ("compare.view_menu", "CompareView"),
            ("compare.changes", "CompareView"),
            ("compare.details", "CompareView"),
            ("compare.toggle_viewed", "CompareStream"),
            ("compare.fold", "CompareStream"),
            ("compare.open_file", "CompareStream"),
            ("compare.search", "CompareStream"),
            ("compare.copy_all_left", "CompareStream"),
            ("compare.copy_all_right", "CompareStream"),
        ]
    );
}

#[test]
fn hidden_compare_commands_stay_out_of_the_lists_and_the_palette() {
    let _hidden = hide_compare();
    assert_eq!(all().filter(|s| s.name.starts_with("compare.")).count(), 0);
    let shown = entries()
        .iter()
        .filter(|entry| entry.spec.name.starts_with("compare."))
        .count();
    assert_eq!(shown, 0);
}

#[test]
fn hidden_compare_commands_still_resolve_by_name_and_action() {
    assert_eq!(
        by_name("compare.exit").map(|spec| spec.action),
        Some(Some("compare::Exit"))
    );
    assert_eq!(
        for_action("compare::Save").map(|spec| spec.name),
        Some("compare.save")
    );
}

const NOT_BUILT: [&str; 9] = [
    "compare.refs",
    "compare.copy_left",
    "compare.copy_right",
    "compare.save",
    "compare.view_bytes",
    "compare.view_text",
    "compare.search",
    "compare.copy_all_left",
    "compare.copy_all_right",
];

fn palette_names() -> Vec<&'static str> {
    entries()
        .iter()
        .filter(|entry| entry.spec.name.starts_with("compare.") && entry.in_palette)
        .map(|entry| entry.spec.name)
        .collect()
}

#[test]
fn built_compare_commands_join_the_lists_and_the_palette() {
    let built: Vec<&str> = compare_names()
        .into_iter()
        .filter(|name| !NOT_BUILT.contains(name))
        .collect();
    let listed: Vec<&str> = all()
        .map(|spec| spec.name)
        .filter(|name| name.starts_with("compare."))
        .collect();
    assert_eq!(listed, built);
    let palette = palette_names();
    assert_eq!(palette.len(), built.len() - 2);
    assert!(!palette.contains(&"compare.enter") && !palette.contains(&"compare.up"));
    assert!(palette.contains(&"compare.with_other_pane"));
}

#[test]
fn commands_still_to_be_built_stay_out_while_compare_is_on() {
    let listed: Vec<&str> = all().map(|spec| spec.name).collect();
    let palette = palette_names();
    for name in NOT_BUILT {
        assert!(
            !listed.contains(&name) && !palette.contains(&name),
            "{name}"
        );
    }
}

#[test]
fn every_compare_command_has_its_label_and_an_action() {
    let broken: Vec<&str> = registry()
        .filter(|spec| spec.name.starts_with("compare."))
        .filter(|spec| !catalog().has(spec.label) || spec.action.is_none())
        .map(|spec| spec.name)
        .collect();
    assert_eq!(broken, Vec::<&str>::new());
}

#[gpui_kit::test]
fn every_compare_action_exists(cx: &mut TestAppContext) {
    let missing: Vec<&str> = cx.update(|cx: &mut App| {
        registry()
            .filter(|spec| spec.name.starts_with("compare."))
            .filter_map(|spec| spec.action)
            .filter(|action| cx.build_action(action, None).is_err())
            .collect()
    });
    assert_eq!(missing, Vec::<&str>::new());
}
