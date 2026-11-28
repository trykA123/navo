use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::app_menu::{Rig, Target, rig};
use crate::settings::{FIELDS, Field, Filter, Section};
use crate::tokens::ThemeChoice;

fn settle_motion(cx: &mut TestAppContext, rig: &Rig) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    if let Some(settings) = rig.settings_window(cx) {
        let _ = cx.update_window(settings, |_, window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    settle_motion(cx, rig);
}

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.view.read_with(cx, |view, _| view.settings.is_open())
}

fn row_of(field: Field) -> usize {
    FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .expect("every field has a row")
}

fn shows(cx: &mut TestAppContext, rig: &Rig, id: impl Into<gpui_kit::ElementId>) -> bool {
    let Some(window) = rig.settings_window(cx) else {
        return false;
    };
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("settings window")
}

fn shows_row(cx: &mut TestAppContext, rig: &Rig, field: Field) -> bool {
    shows(cx, rig, ("settings-row", row_of(field)))
}

fn current_row(cx: &mut TestAppContext, rig: &Rig) -> usize {
    rig.view.read_with(cx, |view, _| view.settings_row())
}

fn find_text(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    let window = rig.settings_window(cx).expect("settings window");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find("settings-find").value().map(str::to_owned)
    })
    .expect("settings window")
}

fn find(cx: &mut TestAppContext, rig: &Rig, text: &str) {
    rig.press_in(cx, Target::Settings, "ctrl-f");
    rig.input_in(cx, Target::Settings, text);
    settle_motion(cx, rig);
}

#[gpui_kit::test]
fn find_filters_rows_by_label(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    assert!(shows_row(cx, &rig, Field::Theme));
    find(cx, &rig, "FOLDERS first");
    assert!(shows_row(cx, &rig, Field::FoldersFirst));
    assert!(!shows_row(cx, &rig, Field::Theme));
    assert!(!shows_row(cx, &rig, Field::ShowSystem));
}

#[gpui_kit::test]
fn find_filters_rows_by_key(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "appearance.density");
    assert!(shows_row(cx, &rig, Field::Density));
    assert!(!shows_row(cx, &rig, Field::Theme));
}

#[gpui_kit::test]
fn find_filters_rows_by_help_text(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "row height");
    assert!(shows_row(cx, &rig, Field::Density));
    assert!(!shows_row(cx, &rig, Field::Theme));
}

#[gpui_kit::test]
fn find_lists_matches_from_every_section_and_hides_empty_headings(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "line numbers");
    assert!(shows_row(cx, &rig, Field::TextLineNumbers));
    assert!(shows(cx, &rig, "settings-group-text and code"));
    assert!(!shows(cx, &rig, "settings-group-window parts"));
    assert!(!shows(cx, &rig, "settings-group-images and markdown"));
    assert!(!shows(cx, &rig, "settings-no-match"));
}

#[gpui_kit::test]
fn find_moves_the_row_to_the_first_match_and_arrows_stay_on_matches(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "files.sort");
    assert_eq!(current_row(cx, &rig), row_of(Field::Sort));
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(current_row(cx, &rig), row_of(Field::Sort));
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(current_row(cx, &rig), row_of(Field::SortDescending));
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(current_row(cx, &rig), row_of(Field::SortDescending));
    rig.press_in(cx, Target::Settings, "up");
    assert_eq!(current_row(cx, &rig), row_of(Field::Sort));
}

#[gpui_kit::test]
fn no_match_shows_a_short_message(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "qqqq");
    assert!(shows(cx, &rig, "settings-no-match"));
    assert!(!shows_row(cx, &rig, Field::Theme));
}

#[gpui_kit::test]
fn escape_clears_the_find_and_a_second_escape_closes(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "appearance.color-theme");
    assert!(!shows_row(cx, &rig, Field::Theme));
    rig.press_in(cx, Target::Settings, "escape");
    settle_motion(cx, &rig);
    assert!(is_open(cx, &rig));
    assert_eq!(find_text(cx, &rig).as_deref(), Some(""));
    assert!(shows_row(cx, &rig, Field::Theme));
    rig.press_in(cx, Target::Settings, "escape");
    assert!(!is_open(cx, &rig));
}

#[gpui_kit::test]
fn typing_in_the_list_does_not_fill_the_find_or_change_rows(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    rig.input_in(cx, Target::Settings, "sort");
    assert_eq!(find_text(cx, &rig).as_deref(), Some(""));
    assert!(shows_row(cx, &rig, Field::Theme));
    assert_eq!(current_row(cx, &rig), 0);
}

#[gpui_kit::test]
fn closing_the_sheet_clears_the_find(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "appearance.color-theme");
    rig.press_in(cx, Target::Settings, "escape");
    rig.press_in(cx, Target::Settings, "escape");
    open(cx, &rig);
    assert_eq!(find_text(cx, &rig).as_deref(), Some(""));
    assert!(shows_row(cx, &rig, Field::Theme));
}

#[gpui_kit::test]
fn keys_change_nothing_when_no_row_matches_the_find(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::Dark);
    open(cx, &rig);
    find(cx, &rig, "zzz");
    let before = rig.view.read_with(cx, |view, _| view.values.clone());
    rig.press_in(cx, Target::Settings, "down");
    for key in ["space", "left", "right", "f8"] {
        rig.press_in(cx, Target::Settings, key);
    }
    settle_motion(cx, &rig);
    assert!(rig.store.changes.lock().expect("changes").is_empty());
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.values.clone()),
        before
    );
}

fn label(cx: &mut TestAppContext, rig: &Rig, id: impl Into<gpui_kit::ElementId>) -> Option<String> {
    let window = rig.settings_window(cx)?;
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .filter(|found| found.visible())
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("settings window")
}

fn page_index(page: Section) -> usize {
    Section::ALL
        .iter()
        .position(|shown| *shown == page)
        .expect("page in Section::ALL")
}

#[gpui_kit::test]
fn nav_shows_page_counts_and_dims_zero_count_pages(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    let appearance = page_index(Section::Appearance);
    let tools = page_index(Section::Tools);
    assert_eq!(
        label(cx, &rig, ("settings-section-count", appearance)),
        None
    );
    assert_eq!(label(cx, &rig, ("settings-section-count", tools)), None);

    find(cx, &rig, "git");
    let hidden = rig.view.read_with(cx, |view, _| view.values.hidden_row());
    let expected_tools = Filter::new("git").hiding(hidden).count_in(Section::Tools);
    assert!(expected_tools > 0);
    assert_eq!(
        label(cx, &rig, ("settings-section-count", appearance)).as_deref(),
        Some("0")
    );
    assert_eq!(
        label(cx, &rig, ("settings-section-count", tools)).as_deref(),
        Some(expected_tools.to_string().as_str())
    );
    assert_eq!(
        label(cx, &rig, "settings-modules").as_deref(),
        Some("Modules")
    );
    assert!(shows(
        cx,
        &rig,
        ("settings-label", row_of(Field::GitBadges))
    ));

    let before = current_row(cx, &rig);
    rig.click_in(cx, Target::Settings, ("settings-section", appearance));
    assert_eq!(current_row(cx, &rig), before);

    rig.press_in(cx, Target::Settings, "escape");
    settle_motion(cx, &rig);
    assert_eq!(
        label(cx, &rig, ("settings-section-count", appearance)),
        None
    );
    rig.click_in(cx, Target::Settings, ("settings-section", appearance));
    assert_eq!(current_row(cx, &rig), row_of(Field::Theme));
}

#[gpui_kit::test]
fn changed_chip_filters_to_changed_rows_and_clears_in_esc_chain_or_on_last_undo(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    let settings = rig.settings_window(cx).expect("settings window");
    assert_eq!(label(cx, &rig, "settings-changed-chip"), None);

    // Change Theme (Appearance) and FoldersFirst (Files).
    rig.press_in(cx, Target::Settings, "right");
    super::settings_sheet::go_to(cx, &rig, Field::FoldersFirst);
    rig.press_in(cx, Target::Settings, "space");
    settle_motion(cx, &rig);

    assert_eq!(
        label(cx, &rig, "settings-changed-chip").as_deref(),
        Some("2 changed")
    );

    // Tab from Find reaches the changed chip, and Enter toggles the filter on.
    rig.press_in(cx, Target::Settings, "ctrl-f");
    cx.update_window(settings, |_, window, cx| window.focus_next(cx))
        .expect("settings window");
    settle_motion(cx, &rig);
    let chip_focused = cx
        .update_window(settings, |_, window, cx| {
            window.render_frame(cx);
            window.find("settings-changed-chip").focused() == Some(true)
        })
        .expect("settings window");
    assert!(chip_focused);

    rig.press_in(cx, Target::Settings, "enter");
    settle_motion(cx, &rig);
    assert!(shows_row(cx, &rig, Field::Theme));
    assert!(shows_row(cx, &rig, Field::FoldersFirst));
    assert!(!shows_row(cx, &rig, Field::Density));
    assert!(!shows_row(cx, &rig, Field::ShowSystem));

    let appearance = page_index(Section::Appearance);
    let files = page_index(Section::Files);
    let window_page = page_index(Section::Window);
    assert_eq!(
        label(cx, &rig, ("settings-section-count", appearance)).as_deref(),
        Some("1")
    );
    assert_eq!(
        label(cx, &rig, ("settings-section-count", files)).as_deref(),
        Some("1")
    );
    assert_eq!(
        label(cx, &rig, ("settings-section-count", window_page)).as_deref(),
        Some("0")
    );
    assert!(shows(cx, &rig, ("settings-section-dot", appearance)));
    assert!(!shows(cx, &rig, ("settings-section-dot", window_page)));
    assert_eq!(
        label(cx, &rig, ("settings-section-count", Section::ALL.len())),
        None
    );

    // 3-step Esc chain: Find words first, then changed-only filter, then close.
    find(cx, &rig, "theme");
    assert!(shows_row(cx, &rig, Field::Theme));
    assert!(!shows_row(cx, &rig, Field::FoldersFirst));

    rig.press_in(cx, Target::Settings, "escape");
    settle_motion(cx, &rig);
    assert!(is_open(cx, &rig));
    assert_eq!(find_text(cx, &rig).as_deref(), Some(""));
    assert!(shows_row(cx, &rig, Field::FoldersFirst));

    rig.press_in(cx, Target::Settings, "escape");
    settle_motion(cx, &rig);
    assert!(is_open(cx, &rig));
    assert!(shows_row(cx, &rig, Field::Density));

    // Turn changed-only back on by clicking the chip, then undo both changes:
    // undoing the last change automatically turns off the filter.
    rig.click_in(cx, Target::Settings, "settings-changed-chip");
    settle_motion(cx, &rig);
    assert!(!shows_row(cx, &rig, Field::Density));

    rig.press_in(cx, Target::Settings, "ctrl-z");
    settle_motion(cx, &rig);
    assert_eq!(
        label(cx, &rig, "settings-changed-chip").as_deref(),
        Some("1 changed")
    );
    assert!(!shows_row(cx, &rig, Field::Density));

    rig.press_in(cx, Target::Settings, "ctrl-z");
    settle_motion(cx, &rig);
    assert_eq!(label(cx, &rig, "settings-changed-chip"), None);
    assert!(shows_row(cx, &rig, Field::Density));

    // Esc now closes Settings.
    rig.press_in(cx, Target::Settings, "escape");
    assert!(!is_open(cx, &rig));
}

#[gpui_kit::test]
fn find_matches_option_labels_and_opens_matching_shortcuts(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);

    find(cx, &rig, "PowerShell");
    assert!(shows_row(cx, &rig, Field::TerminalProvider));
    assert!(!shows_row(cx, &rig, Field::Theme));

    rig.press_in(cx, Target::Settings, "escape");
    settle_motion(cx, &rig);

    find(cx, &rig, "Copy full path");
    let keyboard = page_index(Section::Keyboard);
    let expected = Filter::new("Copy full path")
        .count_in(Section::Keyboard)
        .to_string();
    assert_eq!(
        label(cx, &rig, ("settings-section-count", keyboard)).as_deref(),
        Some(expected.as_str())
    );
    assert!(shows(cx, &rig, "settings-shortcut-hit-files.copy_path"));
    assert_eq!(
        label(cx, &rig, "settings-shortcut-key-files.copy_path").as_deref(),
        Some("F9")
    );

    rig.click_in(
        cx,
        Target::Settings,
        "settings-shortcut-hit-files.copy_path",
    );
    settle_motion(cx, &rig);
    assert!(
        rig.view
            .read_with(cx, |view, _| view.is_showing_shortcuts())
    );
    let settings = rig.settings_window(cx).expect("settings window");
    let selected = cx
        .update_window(settings, |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find("shortcut-files.copy_path")
                .and_then(|found| found.selected())
        })
        .expect("settings window");
    assert_eq!(selected, Some(true));
}

#[gpui_kit::test]
fn reverting_one_of_two_changed_rows_in_changed_only_moves_to_remaining_row(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);

    // Change Theme, Density (Appearance) and FoldersFirst (Files).
    rig.press_in(cx, Target::Settings, "right");
    super::settings_sheet::go_to(cx, &rig, Field::Density);
    rig.press_in(cx, Target::Settings, "right");
    super::settings_sheet::go_to(cx, &rig, Field::FoldersFirst);
    rig.press_in(cx, Target::Settings, "space");
    settle_motion(cx, &rig);

    rig.click_in(cx, Target::Settings, "settings-changed-chip");
    rig.click_in(
        cx,
        Target::Settings,
        ("settings-row", row_of(Field::Density)),
    );
    settle_motion(cx, &rig);
    assert_eq!(current_row(cx, &rig), row_of(Field::Density));

    // Revert Density while changed-only is active: selection moves to an admitted row,
    // and Up/Down navigation still steps between Theme and FoldersFirst.
    rig.press_in(cx, Target::Settings, "f8");
    settle_motion(cx, &rig);
    assert_eq!(current_row(cx, &rig), row_of(Field::Theme));
    assert!(!shows_row(cx, &rig, Field::Density));

    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(current_row(cx, &rig), row_of(Field::FoldersFirst));
    rig.press_in(cx, Target::Settings, "up");
    assert_eq!(current_row(cx, &rig), row_of(Field::Theme));
}
