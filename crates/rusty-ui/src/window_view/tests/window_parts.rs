use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext};
use rusty_core::PaneId;

use super::app_menu::{Rig, rig_full, rig_on};
use super::crash_notice::unclean;
use super::drive_fs::{Setup, drive_window};
use super::jobs::{MB, add, key, open_at, running};
use super::{press, with_fkey_bar};
use crate::app_menu::QuickSettings;
use crate::file_list::SortBy;
use crate::settings::{Change, FIELDS, Field, Value, Values};
use crate::window_view::FerriteWindow;

pub(super) fn off(fields: &[Field]) -> Values {
    fields
        .iter()
        .fold(with_fkey_bar(Values::default()), |values, field| {
            values.with(&Change {
                field: *field,
                value: Value::Flag(false),
            })
        })
}

pub(super) fn every_part() -> Vec<Field> {
    FIELDS
        .iter()
        .map(|spec| spec.field)
        .filter(|field| field.is_window_part())
        .collect()
}

fn pane_of(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> PaneId {
    view.read_with(cx, |view, _| view.browser.state().focused_pane())
}

fn is_drawn(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
    .expect("window")
}

fn drawn_in_list(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> bool {
    let pane = pane_of(cx, &rig.view);
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.within(("list", pane.get())).try_find(id).is_some()
    })
    .expect("window")
}

fn status_label(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    let pane = pane_of(cx, &rig.view);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let status = window.try_find(("status", pane.get()))?;
        status.label().map(str::to_owned)
    })
    .expect("window")
}

fn drawn_with(
    cx: &mut TestAppContext,
    field: Field,
    probe: impl Fn(&mut TestAppContext, &Rig) -> bool,
) {
    let shown = rig_on(cx, with_fkey_bar(Values::default()), None);
    assert!(probe(cx, &shown), "{field:?} is drawn by default");
    let hidden = rig_on(cx, off(&[field]), None);
    assert!(!probe(cx, &hidden), "{field:?} is drawn although hidden");
}

fn pane_probe(id: &'static str) -> impl Fn(&mut TestAppContext, &Rig) -> bool {
    move |cx, rig| {
        let pane = pane_of(cx, &rig.view);
        is_drawn(cx, rig.window, (id, pane.get()))
    }
}

#[gpui_kit::test]
fn a_hidden_tabs_row_with_one_tab_is_not_drawn(cx: &mut TestAppContext) {
    drawn_with(cx, Field::WindowTabsWithOne, pane_probe("tabs"));
}

#[gpui_kit::test]
fn a_hidden_path_bar_is_not_drawn(cx: &mut TestAppContext) {
    drawn_with(cx, Field::WindowPathBar, pane_probe("path"));
}

#[gpui_kit::test]
fn a_hidden_drive_button_is_not_drawn(cx: &mut TestAppContext) {
    drawn_with(cx, Field::WindowDriveButton, pane_probe("drive-pill"));
}

#[gpui_kit::test]
fn a_hidden_title_search_is_not_drawn(cx: &mut TestAppContext) {
    drawn_with(cx, Field::WindowTitleSearch, |cx, rig| {
        is_drawn(cx, rig.window, "title-bar-search")
    });
}

#[gpui_kit::test]
fn a_hidden_fkey_bar_is_not_drawn(cx: &mut TestAppContext) {
    drawn_with(cx, Field::WindowFkeyBar, |cx, rig| {
        is_drawn(cx, rig.window, "fkeys")
    });
}

#[gpui_kit::test]
fn the_fkey_bar_is_not_drawn_by_default(cx: &mut TestAppContext) {
    let rig = rig_on(cx, Values::default(), None);
    assert!(!is_drawn(cx, rig.window, "fkeys"));
}

#[gpui_kit::test]
fn hidden_icons_are_not_drawn(cx: &mut TestAppContext) {
    drawn_with(cx, Field::WindowIcons, |cx, rig| {
        let pane = pane_of(cx, &rig.view);
        cx.update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            let mut list = window.within(("list", pane.get()));
            list.within(("row", 0usize)).try_find("row-icon").is_some()
        })
        .expect("window")
    });
}

#[gpui_kit::test]
fn hidden_columns_are_not_drawn(cx: &mut TestAppContext) {
    for (field, index) in [
        (Field::WindowExtColumn, 1usize),
        (Field::WindowSizeColumn, 2),
        (Field::WindowDateColumn, 3),
    ] {
        drawn_with(cx, field, move |cx, rig| {
            drawn_in_list(cx, rig, ("sort", index))
        });
    }
}

#[gpui_kit::test]
fn the_name_column_is_always_drawn(cx: &mut TestAppContext) {
    let rig = rig_on(cx, off(&every_part()), None);
    assert!(drawn_in_list(cx, &rig, ("sort", 0usize)));
    assert!(drawn_in_list(cx, &rig, ("row", 0usize)));
}

#[gpui_kit::test]
fn hidden_footer_parts_leave_the_status_text(cx: &mut TestAppContext) {
    let all = rig_on(cx, Values::default(), None);
    let text = status_label(cx, &all).expect("a footer");
    assert!(text.contains("free") && text.contains("folder"), "{text}");
    let no_free = rig_on(cx, off(&[Field::WindowFreeSpace]), None);
    let text = status_label(cx, &no_free).expect("a footer");
    assert!(!text.contains("free") && text.contains("folder"), "{text}");
    let no_contents = rig_on(cx, off(&[Field::WindowContents]), None);
    let text = status_label(cx, &no_contents).expect("a footer");
    assert!(text.contains("free") && !text.contains("folder"), "{text}");
}

#[gpui_kit::test]
fn hidden_marked_totals_leave_the_footer_when_something_is_marked(cx: &mut TestAppContext) {
    let all = rig_on(cx, Values::default(), None);
    press(cx, all.window, &all.view, "space");
    assert!(status_label(cx, &all).is_some_and(|text| text.contains("marked")));
    let hidden = rig_on(cx, off(&[Field::WindowMarkedTotals]), None);
    press(cx, hidden.window, &hidden.view, "space");
    assert!(status_label(cx, &hidden).is_some_and(|text| !text.contains("marked")));
}

#[gpui_kit::test]
fn a_footer_with_every_part_hidden_is_not_drawn(cx: &mut TestAppContext) {
    let parts = [
        Field::WindowFreeSpace,
        Field::WindowMarkedTotals,
        Field::WindowContents,
    ];
    let rig = rig_on(cx, off(&parts), None);
    assert_eq!(status_label(cx, &rig), None);
}

#[gpui_kit::test]
fn ctrl_l_edits_the_path_while_the_path_bar_is_hidden(cx: &mut TestAppContext) {
    let rig = rig_on(cx, off(&[Field::WindowPathBar]), None);
    let pane = pane_of(cx, &rig.view);
    assert!(!is_drawn(cx, rig.window, ("path", pane.get())));
    rig.press(cx, "ctrl-l");
    let edit = rig
        .view
        .read_with(cx, |view, _| view.path_edit.as_ref().map(|edit| edit.id));
    let id = edit.expect("the path edit opens");
    assert!(is_drawn(cx, rig.window, ("path-input", id)));
    rig.press(cx, "escape");
    assert!(!is_drawn(cx, rig.window, ("path-input", id)));
    assert!(!is_drawn(cx, rig.window, ("path", pane.get())));
}

#[gpui_kit::test]
fn the_f_keys_work_while_the_fkey_bar_is_hidden(cx: &mut TestAppContext) {
    let rig = rig_on(cx, off(&[Field::WindowFkeyBar]), None);
    assert!(!is_drawn(cx, rig.window, "fkeys"));
    rig.press(cx, "f7");
    assert!(rig.view.read_with(cx, |view, _| view.prompt.is_some()));
}

#[gpui_kit::test]
fn alt_f1_opens_the_drive_list_while_the_drive_button_is_hidden(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    rig.view.update(cx, |view, _| {
        view.values = off(&[Field::WindowDriveButton, Field::WindowTabsWithOne]);
    });
    let pane = rig.panes(cx)[0];
    press(cx, rig.window, &rig.view, "alt-f1");
    rig.frame(cx, |window, _| {
        assert!(window.try_find(("drive-pill", pane.get())).is_none());
        assert!(window.try_find(("tabs", pane.get())).is_none());
        let drives = window.within(("drive-menu", pane.get()));
        assert!(drives.try_find(("drive-row", 0usize)).is_some());
    });
}

#[gpui_kit::test]
fn ctrl_t_brings_the_tabs_row_back_while_one_tab_is_hidden(cx: &mut TestAppContext) {
    let rig = rig_on(cx, off(&[Field::WindowTabsWithOne]), None);
    let pane = pane_of(cx, &rig.view);
    assert!(!is_drawn(cx, rig.window, ("tabs", pane.get())));
    rig.press(cx, "ctrl-t");
    assert!(is_drawn(cx, rig.window, ("tabs", pane.get())));
}

#[gpui_kit::test]
fn the_sort_keys_still_sort_by_a_hidden_column(cx: &mut TestAppContext) {
    let parts = [
        Field::WindowExtColumn,
        Field::WindowSizeColumn,
        Field::WindowDateColumn,
    ];
    let rig = rig_on(cx, off(&parts), None);
    for (key, by) in [
        ("ctrl-f6", SortBy::Size),
        ("ctrl-f4", SortBy::Ext),
        ("ctrl-f5", SortBy::Date),
    ] {
        rig.press(cx, key);
        let sorted = rig.view.read_with(cx, |view, _| {
            let tab = view.browser.state().active_tab().id();
            view.browser.sort_key(tab)
        });
        assert_eq!(sorted.by, by, "{key}");
        let pane = pane_of(cx, &rig.view);
        cx.update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            let mut list = window.within(("list", pane.get()));
            let name = list.within(("sort", 0usize));
            assert!(name.try_find("sort-hidden").is_some(), "{key}");
        })
        .expect("window");
    }
}

#[gpui_kit::test]
fn an_error_still_shows_with_every_part_hidden(cx: &mut TestAppContext) {
    let rig = rig_full(
        cx,
        QuickSettings::default(),
        Some("navo.toml is invalid at line 2"),
    );
    rig.view
        .update(cx, |view, _| view.values = off(&every_part()));
    let pane = pane_of(cx, &rig.view);
    let line = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(("pane-error", pane.get()))
                .and_then(|found| found.label().map(str::to_owned))
        })
        .expect("window");
    assert_eq!(line.as_deref(), Some("navo.toml is invalid at line 2"));
}

#[gpui_kit::test]
fn the_crash_notice_still_shows_with_every_part_hidden(cx: &mut TestAppContext) {
    let rig = unclean(cx);
    rig.view
        .update(cx, |view, _| view.values = off(&every_part()));
    assert!(rig.is_shown(cx));
}

#[gpui_kit::test]
fn a_job_shows_in_the_activity_strip_with_the_fkey_bar_hidden(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    view.update(cx, |view, _| view.values = off(&[Field::WindowFkeyBar]));
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "ctrl-j");
    let (title, right, edge) = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let title = window
                .find(("activity-title", 1u64))
                .label()
                .map(str::to_owned);
            let strip = window.find("activity-strip").bounds();
            (title, strip.right(), window.viewport_size().width)
        })
        .expect("window");
    assert!(title.is_some_and(|title| title.starts_with("Copying 12 files")));
    assert!(edge - right < gpui_kit::px(1.), "{right:?} of {edge:?}");
    assert!(!is_drawn(cx, window, "fkeys"));
}
