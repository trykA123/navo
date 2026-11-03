use gpui_kit::{App, TestAppContext};
use navo_l10n::catalog;

use super::{all, by_name, entries, for_action, registry};
use crate::compare_visible::show_compare;

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
            ("compare.save", "CompareView"),
            ("compare.view_bytes", "CompareView"),
            ("compare.view_text", "CompareView"),
        ]
    );
}

#[test]
fn hidden_compare_commands_stay_out_of_the_lists_and_the_palette() {
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

#[test]
fn visible_compare_commands_join_the_lists_and_the_palette() {
    let _shown = show_compare();
    let names = compare_names();
    let listed: Vec<&str> = all().map(|spec| spec.name).collect();
    assert!(names.iter().all(|name| listed.contains(name)));
    let palette: Vec<&str> = entries()
        .iter()
        .filter(|entry| entry.spec.name.starts_with("compare.") && entry.in_palette)
        .map(|entry| entry.spec.name)
        .collect();
    assert_eq!(palette.len(), names.len() - 2);
    assert!(!palette.contains(&"compare.enter") && !palette.contains(&"compare.up"));
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
