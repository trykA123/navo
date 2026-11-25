use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};

use super::app_menu::{Rig, rig};
use super::drive_fs::{Setup, drive_window};
use super::press;
use crate::app_menu::{Item, MenuCommand, index_of};
use crate::settings::{Change, FIELDS, Field, Section, Value};
use crate::tokens::{self, Density, ThemeChoice};

fn settle_motion(cx: &mut TestAppContext, rig: &Rig) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

pub(super) fn open(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    settle_motion(cx, rig);
}

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.view.read_with(cx, |view, _| view.settings.is_open())
}

pub(super) fn label(
    cx: &mut TestAppContext,
    rig: &Rig,
    id: impl Into<ElementId>,
) -> Option<String> {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .filter(|found| found.visible())
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

pub(super) fn shows(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("window")
}

pub(super) fn sheet_has_focus(cx: &mut TestAppContext, rig: &Rig) -> bool {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.find("settings-body").focused() == Some(true)
    })
    .expect("window")
}

fn row_of(field: Field) -> usize {
    FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .expect("every field has a row")
}

pub(super) fn go_to(cx: &mut TestAppContext, rig: &Rig, field: Field) {
    for _ in 0..FIELDS.len() * 2 {
        if rig.view.read_with(cx, |view, _| view.settings_row()) == row_of(field) {
            return;
        }
        rig.press(cx, "down");
    }
    panic!("{field:?} is not reachable with the down key");
}

pub(super) fn last_change(rig: &Rig) -> Option<Change> {
    rig.store.changes.lock().expect("changes").last().cloned()
}

fn density(cx: &mut TestAppContext) -> Density {
    cx.read(|cx| tokens::current(cx).density)
}

#[gpui_kit::test]
fn ctrl_comma_opens_settings_as_a_centred_dialog_with_focus_inside(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert!(!shows(cx, &rig, "settings-sheet"));
    open(cx, &rig);
    assert!(is_open(cx, &rig));
    assert!(shows(cx, &rig, "settings-sheet"));
    assert!(sheet_has_focus(cx, &rig));
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let sheet = window.find("settings-sheet").bounds().center().x;
        let middle = window.viewport_size().width / 2.;
        assert!(
            (sheet - middle).abs() <= gpui_kit::px(1.),
            "{sheet:?} vs {middle:?}"
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn escape_closes_settings_and_gives_focus_back_to_the_list(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    rig.press(cx, "down");
    rig.press(cx, "escape");
    assert!(!is_open(cx, &rig));
    assert!(rig.pane_has_focus(cx));
    settle_motion(cx, &rig);
    assert!(!shows(cx, &rig, "settings-sheet"));
}

#[gpui_kit::test]
fn the_all_settings_row_of_the_app_menu_opens_settings(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    let row = index_of(Item::Row(MenuCommand::AllSettings)).expect("all settings row");
    rig.click(cx, ("app-menu-row", row));
    settle_motion(cx, &rig);
    assert!(!rig.is_open(cx));
    assert!(is_open(cx, &rig));
    assert!(sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn the_focused_row_shows_its_toml_key_and_down_moves_on(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    assert_eq!(
        label(cx, &rig, "settings-key").as_deref(),
        Some("appearance.mode")
    );
    rig.press(cx, "down");
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.settings_row()),
        row_of(Field::ColorTheme)
    );
    assert_eq!(
        label(cx, &rig, "settings-key").as_deref(),
        Some("appearance.color-theme")
    );
    rig.press(cx, "up");
    rig.press(cx, "up");
    assert_eq!(rig.view.read_with(cx, |view, _| view.settings_row()), 0);
}

#[gpui_kit::test]
fn arrows_change_the_density_live_and_save_only_that_key(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "right");
    assert_eq!(density(cx), Density::Comfortable);
    assert_eq!(
        last_change(&rig),
        Some(Change::density(Density::Comfortable))
    );
    rig.press(cx, "left");
    rig.press(cx, "left");
    assert_eq!(density(cx), Density::Compact);
    assert_eq!(last_change(&rig), Some(Change::density(Density::Compact)));
    assert!(sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn space_on_hidden_files_hides_them_in_the_live_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    rig.press(cx, "tab");
    rig.press(cx, "tab");
    assert_eq!(
        label(cx, &rig, "settings-key").as_deref(),
        Some("files.hidden-files")
    );
    rig.press(cx, "space");
    let panes = rig.pane_names(cx);
    assert!(
        panes
            .iter()
            .all(|names| !names.contains(&"b-secret.txt".to_owned()))
    );
    assert_eq!(last_change(&rig), Some(Change::show_hidden(false)));
}

#[gpui_kit::test]
fn space_flips_a_toggle(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::FoldersFirst);
    rig.press(cx, "space");
    let change = Change {
        field: Field::FoldersFirst,
        value: Value::Flag(false),
    };
    assert_eq!(last_change(&rig), Some(change));
}

#[gpui_kit::test]
fn tab_walks_the_pages_and_focus_stays_in_the_sheet(cx: &mut TestAppContext) {
    let _hidden = crate::compare_visible::hide_compare();
    walk_pages(cx);
}

#[gpui_kit::test]
fn tab_walks_the_same_pages_while_compare_is_visible(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    walk_pages(cx);
}

fn walk_pages(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    let pages = [
        "Window",
        "Files",
        "Copy and delete",
        "Search",
        "Viewers",
        "Tools",
        "Tray and clipboard",
        "Keyboard",
        "About",
        "Appearance",
    ];
    for (step, name) in pages.iter().enumerate() {
        rig.press(cx, "tab");
        assert!(sheet_has_focus(cx, &rig), "after tab {step}");
        let current = (0..=Section::ALL.len()).find(|index| {
            cx.update_window(rig.window, |_, window, cx| {
                window.render_frame(cx);
                window.find(("settings-section", *index)).selected() == Some(true)
            })
            .expect("window")
        });
        let shown = current.and_then(|index| label(cx, &rig, ("settings-section", index)));
        assert_eq!(shown.as_deref(), Some(*name));
    }
    for _ in 0..9 {
        rig.press(cx, "shift-tab");
    }
    assert_eq!(
        label(cx, &rig, "settings-key").as_deref(),
        Some("window.view.tabs-with-one")
    );
}

#[gpui_kit::test]
fn the_display_section_starts_with_the_window_group_and_flips_a_part(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::WindowPathBar);
    assert_eq!(
        label(cx, &rig, "settings-group-window parts").as_deref(),
        Some("Window parts")
    );
    assert_eq!(
        label(cx, &rig, "settings-key").as_deref(),
        Some("window.view.path-bar")
    );
    rig.press(cx, "space");
    let change = Change {
        field: Field::WindowPathBar,
        value: Value::Flag(false),
    };
    assert_eq!(last_change(&rig), Some(change));
}

#[gpui_kit::test]
fn the_display_section_has_markdown_and_preview_groups_whose_toggles_save(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    for (field, group, key) in [
        (
            Field::MarkdownPreviewSplit,
            "Images and Markdown",
            "markdown.view.preview-split",
        ),
        (
            Field::PreviewTypeBars,
            "Folder preview",
            "preview.view.type-bars",
        ),
        (
            Field::PreviewLargestFiles,
            "Folder preview",
            "preview.view.largest-files",
        ),
    ] {
        if field == Field::MarkdownPreviewSplit {
            go_to(cx, &rig, field);
        } else {
            rig.press(cx, "down");
        }
        assert_eq!(
            label(cx, &rig, format!("settings-group-{}", group.to_lowercase())).as_deref(),
            Some(group)
        );
        assert_eq!(label(cx, &rig, "settings-key").as_deref(), Some(key));
        rig.press(cx, "space");
        let change = Change {
            field,
            value: Value::Flag(false),
        };
        assert_eq!(last_change(&rig), Some(change));
    }
}

#[gpui_kit::test]
fn ctrl_z_undoes_the_last_change_and_saves_the_old_value(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.store
        .present
        .lock()
        .expect("present")
        .push(Field::Density);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "left");
    assert_eq!(density(cx), Density::Compact);
    rig.press(cx, "up");
    rig.press(cx, "ctrl-z");
    assert_eq!(density(cx), Density::Normal);
    assert_eq!(last_change(&rig), Some(Change::density(Density::Normal)));
    assert_eq!(rig.view.read_with(cx, |view, _| view.settings_row()), 1);
    let saves = rig.store.changes.lock().expect("changes").len();
    rig.press(cx, "ctrl-z");
    assert_eq!(rig.store.changes.lock().expect("changes").len(), saves);
}

#[gpui_kit::test]
fn undo_lasts_only_while_the_sheet_is_open(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "left");
    rig.press(cx, "escape");
    open(cx, &rig);
    rig.press(cx, "ctrl-z");
    assert_eq!(density(cx), Density::Compact);
}

#[gpui_kit::test]
fn settings_replace_the_preview_drawer_until_they_close(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f3");
    assert!(rig.view.read_with(cx, |view, _| view.preview.is_open()));
    open(cx, &rig);
    assert!(is_open(cx, &rig));
    assert!(!rig.view.read_with(cx, |view, _| view.preview.is_open()));
    assert!(!shows(cx, &rig, "preview-sheet"));
    rig.press(cx, "escape");
    settle_motion(cx, &rig);
    assert!(rig.view.read_with(cx, |view, _| view.preview.is_open()));
    assert!(shows(cx, &rig, "preview-sheet"));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn a_failed_save_shows_a_line_in_the_sheet(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    *rig.store.failure.lock().expect("failure") = Some("navo.toml: disk full".to_owned());
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "right");
    assert_eq!(
        label(cx, &rig, "settings-error").as_deref(),
        Some("navo.toml: disk full")
    );
    assert_eq!(density(cx), Density::Normal);
}

#[gpui_kit::test]
fn enter_edits_a_text_row_and_saves_it(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Font);
    rig.press(cx, "enter");
    assert!(shows(cx, &rig, "settings-input"));
    rig.press(cx, "ctrl-a");
    cx.update_window(rig.window, |_, window, cx| window.input("Archivo", cx))
        .expect("window");
    rig.press(cx, "enter");
    assert!(!shows(cx, &rig, "settings-input"));
    assert_eq!(
        last_change(&rig),
        Some(Change::text(Field::Font, "Archivo"))
    );
    assert!(sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn escape_in_a_text_row_cancels_and_keeps_the_sheet(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Font);
    rig.press(cx, "enter");
    rig.press(cx, "escape");
    assert!(is_open(cx, &rig));
    assert!(!shows(cx, &rig, "settings-input"));
    assert!(sheet_has_focus(cx, &rig));
    assert!(rig.store.changes.lock().expect("changes").is_empty());
}

#[gpui_kit::test]
fn a_font_size_that_is_not_a_number_is_refused(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::FontSize);
    rig.press(cx, "enter");
    rig.press(cx, "ctrl-a");
    cx.update_window(rig.window, |_, window, cx| window.input("big", cx))
        .expect("window");
    rig.press(cx, "enter");
    assert!(label(cx, &rig, "settings-edit-error").is_some());
    assert!(rig.store.changes.lock().expect("changes").is_empty());
}

#[gpui_kit::test]
fn clicking_a_picture_picks_that_theme(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    rig.click(cx, ("settings-theme", 2usize));
    assert_eq!(last_change(&rig), Some(Change::theme(ThemeChoice::Dark)));
    assert_eq!(rig.mode(cx), tokens::Mode::Dark);
}

#[gpui_kit::test]
fn undo_of_a_key_that_was_not_in_the_file_unsets_it(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "left");
    rig.press(cx, "right");
    rig.press(cx, "ctrl-z");
    let undo_once = Change::density(Density::Compact);
    assert_eq!(last_change(&rig), Some(undo_once));
    rig.press(cx, "ctrl-z");
    let unset = Change {
        field: Field::Density,
        value: Value::Unset,
    };
    assert_eq!(last_change(&rig), Some(unset));
    assert_eq!(density(cx), Density::Normal);
}

#[gpui_kit::test]
fn undo_while_a_save_is_in_flight_writes_the_old_value(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::FoldersFirst);
    cx.update_window(rig.window, |_, window, cx| {
        window.press("space", cx);
        window.press("ctrl-z", cx);
    })
    .expect("window");
    cx.run_until_parked();
    let off = Change {
        field: Field::FoldersFirst,
        value: Value::Flag(true),
    };
    assert_eq!(last_change(&rig), Some(off));
    let shown = rig
        .view
        .read_with(cx, |view, _| view.values.get(Field::FoldersFirst).clone());
    assert_eq!(shown, Value::Flag(true));
}

#[gpui_kit::test]
fn ctrl_comma_closes_the_open_app_menu_and_opens_settings(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    assert!(rig.is_open(cx));
    open(cx, &rig);
    assert!(!rig.is_open(cx));
    assert!(is_open(cx, &rig));
    assert!(sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn ctrl_comma_closes_the_drive_menu(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    press(cx, rig.window, &rig.view, "alt-f1");
    assert!(rig.view.read_with(cx, |view, _| view.drive_menu.is_some()));
    press(cx, rig.window, &rig.view, "ctrl-,");
    assert!(rig.view.read_with(cx, |view, _| view.drive_menu.is_none()));
    assert!(rig.view.read_with(cx, |view, _| view.settings.is_open()));
}

#[gpui_kit::test]
fn ctrl_comma_closes_the_path_editor(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "alt-d");
    assert!(rig.view.read_with(cx, |view, _| view.path_edit.is_some()));
    open(cx, &rig);
    assert!(rig.view.read_with(cx, |view, _| view.path_edit.is_none()));
    assert!(is_open(cx, &rig));
    assert!(sheet_has_focus(cx, &rig));
}

fn command_center_shows(cx: &mut TestAppContext, rig: &Rig) -> bool {
    settle_motion(cx, rig);
    shows(cx, rig, "command-center")
}

#[gpui_kit::test]
fn ctrl_comma_closes_the_command_center_and_focuses_settings(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "ctrl-k");
    assert!(command_center_shows(cx, &rig));
    open(cx, &rig);
    assert!(!command_center_shows(cx, &rig));
    assert!(is_open(cx, &rig));
    assert!(sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn the_command_center_closes_back_into_the_open_settings(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "down");
    open(cx, &rig);
    rig.press(cx, "ctrl-k");
    assert!(command_center_shows(cx, &rig));
    rig.press(cx, "escape");
    assert!(!command_center_shows(cx, &rig));
    assert!(is_open(cx, &rig));
    assert!(sheet_has_focus(cx, &rig));
    rig.press(cx, "f8");
    assert!(
        rig.view
            .read_with(cx, |view, _| view.browser.pending().is_none())
    );
}

#[gpui_kit::test]
fn the_display_section_groups_the_viewer_parts_and_flips_one(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::TextLineEnds);
    assert_eq!(
        label(cx, &rig, "settings-group-text and code").as_deref(),
        Some("Text and code")
    );
    assert_eq!(
        label(cx, &rig, "settings-key").as_deref(),
        Some("text.view.line-ends")
    );
    rig.press(cx, "space");
    let change = Change {
        field: Field::TextLineEnds,
        value: Value::Flag(true),
    };
    assert_eq!(last_change(&rig), Some(change));
    rig.press(cx, "down");
    assert_eq!(
        label(cx, &rig, "settings-key").as_deref(),
        Some("text.view.syntax-theme")
    );
    rig.press(cx, "down");
    assert_eq!(
        label(cx, &rig, "settings-group-images and markdown").as_deref(),
        Some("Images and Markdown")
    );
}

#[gpui_kit::test]
fn external_terminal_renders_as_a_dropdown_without_overflowing(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::TerminalProvider);
    let row = row_of(Field::TerminalProvider);
    let dropdown_id = ("settings-dropdown", Field::TerminalProvider.index());
    assert!(
        shows(cx, &rig, dropdown_id),
        "dropdown trigger should render"
    );
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let row_bounds = window.find(("settings-row", row)).bounds();
        let trigger_bounds = window.find(dropdown_id).bounds();
        assert!(
            window
                .within(("settings-row", row))
                .try_find(("settings-segment", 0usize))
                .is_none(),
            "long choice should not render segments"
        );
        assert!(
            trigger_bounds.left() >= row_bounds.left()
                && trigger_bounds.right() <= row_bounds.right(),
            "dropdown {trigger_bounds:?} overflows row {row_bounds:?}"
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn dropdown_opens_on_click_and_picks_an_option(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::TerminalProvider);
    let trigger = ("settings-dropdown", Field::TerminalProvider.index());
    let menu = ("settings-dropdown-menu", Field::TerminalProvider.index());
    rig.click(cx, trigger);
    assert!(shows(cx, &rig, menu));
    for (index, expected) in [
        "System default",
        "Windows Terminal",
        "PowerShell",
        "Command Prompt",
        "Custom",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            label(cx, &rig, ("settings-dropdown-option", index)).as_deref(),
            Some(*expected)
        );
    }
    rig.click(cx, ("settings-dropdown-option", 2usize));
    assert!(!shows(cx, &rig, menu));
    assert_eq!(
        last_change(&rig),
        Some(Change {
            field: Field::TerminalProvider,
            value: Value::Pick(2),
        })
    );
    assert_eq!(label(cx, &rig, trigger).as_deref(), Some("PowerShell"));
}

#[gpui_kit::test]
fn dropdown_keyboard_open_arrows_choose_escape_and_left_right(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::TerminalProvider);
    let menu = ("settings-dropdown-menu", Field::TerminalProvider.index());
    rig.press(cx, "enter");
    assert!(shows(cx, &rig, menu));
    rig.press(cx, "escape");
    assert!(!shows(cx, &rig, menu));
    assert!(is_open(cx, &rig));
    assert!(sheet_has_focus(cx, &rig));
    rig.press(cx, "space");
    assert!(shows(cx, &rig, menu));
    rig.press(cx, "down");
    rig.press(cx, "enter");
    assert!(!shows(cx, &rig, menu));
    assert_eq!(
        last_change(&rig),
        Some(Change {
            field: Field::TerminalProvider,
            value: Value::Pick(1),
        })
    );
    rig.press(cx, "right");
    assert_eq!(
        last_change(&rig),
        Some(Change {
            field: Field::TerminalProvider,
            value: Value::Pick(2),
        })
    );
    rig.press(cx, "left");
    assert_eq!(
        last_change(&rig),
        Some(Change {
            field: Field::TerminalProvider,
            value: Value::Pick(1),
        })
    );
}

#[gpui_kit::test]
fn switching_pages_dismisses_an_open_dropdown(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::TerminalProvider);
    let menu = ("settings-dropdown-menu", Field::TerminalProvider.index());
    rig.press(cx, "enter");
    assert!(shows(cx, &rig, menu));
    rig.press(cx, "ctrl-1");
    rig.press(cx, "ctrl-7");
    assert!(!shows(cx, &rig, menu));
    rig.press(cx, "enter");
    assert!(shows(cx, &rig, menu));
    rig.press(cx, "tab");
    rig.press(cx, "shift-tab");
    assert!(!shows(cx, &rig, menu));
}

#[gpui_kit::test]
fn short_choices_still_render_as_segments(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::HiddenFiles);
    let row = row_of(Field::HiddenFiles);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .within(("settings-row", row))
                .try_find(("settings-segment", 0usize))
                .is_some()
        );
        assert!(
            window
                .try_find(("settings-dropdown", Field::HiddenFiles.index()))
                .is_none()
        );
    })
    .expect("window");
}
