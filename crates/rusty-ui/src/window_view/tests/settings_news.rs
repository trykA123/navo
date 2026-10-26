use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Context, TestAppContext, Window};

use super::app_menu::{Rig, rig};
use crate::app_menu::QuickSettings;
use crate::settings::{Change, FIELDS, Field, Values};
use crate::tokens::{self, Density, ThemeChoice};
use crate::window_view::{FerriteWindow, SettingsNews};

const BROKEN: &str = "navo.toml: invalid settings at line 3, column 9: expected a value";

fn settle_motion(cx: &mut TestAppContext, rig: &Rig) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

fn send(cx: &mut TestAppContext, rig: &Rig, news: SettingsNews) {
    assert!(rig.news.send(news));
    cx.run_until_parked();
}

fn on_disk(density: Density) -> SettingsNews {
    applied(at_start().with(&Change::density(density)), 1)
}

fn applied(values: Values, generation: u64) -> SettingsNews {
    SettingsNews::Applied { values, generation }
}

fn at_start() -> Values {
    super::app_menu::values_of(QuickSettings {
        theme: ThemeChoice::System,
        density: Density::Normal,
        show_hidden: true,
    })
}

fn density(cx: &mut TestAppContext) -> Density {
    cx.read(|cx| tokens::current(cx).density)
}

fn applies(cx: &mut TestAppContext, rig: &Rig) -> usize {
    rig.view.read_with(cx, |view, _| view.saver.disk_applies)
}

fn row_value(cx: &mut TestAppContext, rig: &Rig, field: Field) -> Option<String> {
    let row = FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .expect("row");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .find(("settings-row", row))
            .value()
            .map(str::to_owned)
    })
    .expect("window")
}

fn status(cx: &mut TestAppContext, rig: &Rig) -> String {
    let pane = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let status = window.find(("status", pane.get()));
        status.label().unwrap_or_default().to_owned()
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_open_sheet_shows_a_value_edited_on_disk(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "ctrl-,");
    settle_motion(cx, &rig);
    assert_eq!(
        row_value(cx, &rig, Field::Density).as_deref(),
        Some("Normal")
    );
    send(cx, &rig, on_disk(Density::Comfortable));
    assert_eq!(
        row_value(cx, &rig, Field::Density).as_deref(),
        Some("Comfortable")
    );
    assert_eq!(density(cx), Density::Comfortable);
    assert_eq!(applies(cx, &rig), 1);
    assert!(rig.store.changes.lock().expect("changes").is_empty());
}

#[gpui_kit::test]
fn a_broken_file_keeps_the_values_and_its_line_shows_until_fixed(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, on_disk(Density::Compact));
    send(cx, &rig, SettingsNews::Invalid(BROKEN.to_owned()));
    let shown = status(cx, &rig);
    assert!(shown.starts_with(BROKEN), "{shown}");
    assert_eq!(density(cx), Density::Compact);
    send(cx, &rig, on_disk(Density::Compact));
    let shown = status(cx, &rig);
    assert!(!shown.contains(BROKEN), "{shown}");
    assert_eq!(applies(cx, &rig), 1);
}

#[gpui_kit::test]
fn the_echo_of_a_sheet_change_applies_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "ctrl-,");
    settle_motion(cx, &rig);
    rig.press(cx, "down");
    rig.press(cx, "right");
    assert_eq!(density(cx), Density::Comfortable);
    send(cx, &rig, on_disk(Density::Comfortable));
    assert_eq!(applies(cx, &rig), 0);
    assert_eq!(rig.store.changes.lock().expect("changes").len(), 1);
}

fn act(
    cx: &mut TestAppContext,
    rig: &Rig,
    work: impl FnOnce(&mut FerriteWindow, &mut Window, &mut Context<FerriteWindow>),
) {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| work(view, window, cx));
    })
    .expect("window");
}

fn shown(cx: &mut TestAppContext, rig: &Rig) -> Values {
    rig.view.read_with(cx, |view, _| view.values.clone())
}

#[gpui_kit::test]
fn a_reload_while_a_save_is_in_flight_never_undoes_the_change_on_screen(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    act(cx, &rig, |view, window, cx| {
        view.change_setting(Change::theme(ThemeChoice::Dark), window, cx);
        view.take_settings(applied(at_start(), 1), window, cx);
        assert_eq!(view.values.theme(), ThemeChoice::Dark);
    });
    assert_eq!(applies(cx, &rig), 0);
    cx.run_until_parked();
    assert_eq!(shown(cx, &rig).theme(), ThemeChoice::Dark);
    assert_eq!(rig.store.changes.lock().expect("changes").len(), 1);
}

#[gpui_kit::test]
fn a_reload_older_than_a_finished_save_is_dropped(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    act(cx, &rig, |view, window, cx| {
        view.change_setting(Change::theme(ThemeChoice::Dark), window, cx);
    });
    cx.run_until_parked();
    assert_eq!(shown(cx, &rig).theme(), ThemeChoice::Dark);
    send(cx, &rig, applied(at_start(), 0));
    assert_eq!(shown(cx, &rig).theme(), ThemeChoice::Dark);
    assert_eq!(applies(cx, &rig), 0);
}

#[gpui_kit::test]
fn a_hand_edit_newer_than_a_save_report_stays_on_screen(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    let edited = at_start()
        .with(&Change::theme(ThemeChoice::Dark))
        .with(&Change::density(Density::Compact));
    act(cx, &rig, |view, window, cx| {
        view.change_setting(Change::theme(ThemeChoice::Dark), window, cx);
        view.take_settings(applied(edited.clone(), 7), window, cx);
    });
    cx.run_until_parked();
    let values = shown(cx, &rig);
    assert_eq!(values.theme(), ThemeChoice::Dark);
    assert_eq!(values.density(), Density::Compact);
    assert_eq!(density(cx), Density::Compact);
}
