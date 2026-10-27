use std::sync::Arc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Entity, FontWeight, Hsla, Modifiers, MouseButton, Pixels, Point,
    TestAppContext, TextRun, Window, point, px,
};
use rusty_core::{AppState, PaneId, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry};

use super::layout_widths::{
    Box2, PaneProbe, ROWS, drop_order_breaks, lined_up, outside, panes, probe_pane,
};
use super::{open_options_sized, options_for};
use crate::format;
use crate::settings::{Change, Field, Value, Values};
use crate::test_pointer::{dispatch, drag, up};
use crate::tokens;
use crate::window_view::FerriteWindow;
use crate::window_view::keys::ResetColumnWidths;

const FOLDER: &str = "home/vehicle-logs";
const MODIFIED_MS: i64 = 1_791_920_400_000;
const DAY_MS: i64 = 86_400_000;
const DATE: usize = 3;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    pane: PaneId,
}

fn modified(row: usize) -> i64 {
    MODIFIED_MS + row as i64 * DAY_MS * 37
}

fn dated_fs() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder(FOLDER);
    for row in 0..ROWS {
        fs.add_entry(
            FOLDER,
            FsEntry {
                name: ProviderPath::from_text(&format!("trace-{row:02}.blf")),
                kind: EntryKind::File,
                size: 4_000_000_000 + row as u64 * 12_345_679,
                modified_ms: Some(modified(row)),
                attributes: Attributes::default(),
            },
        );
    }
    Arc::new(fs)
}

fn setting(field: Field, number: i64) -> Change {
    Change {
        field,
        value: Value::Number(number),
    }
}

fn open_with(cx: &mut TestAppContext, changes: &[Change], width: f32) -> Rig {
    let fs = dated_fs();
    let state = AppState::new(fs.location(FOLDER));
    let mut options = options_for(fs, state);
    options.settings.values = changes
        .iter()
        .fold(options.settings.values, |values: Values, change| {
            values.with(change)
        });
    let (window, view) = open_options_sized(cx, options, px(width));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("window");
    let (_, pane) = panes(cx, &view);
    Rig { window, view, pane }
}

fn probe(cx: &mut TestAppContext, rig: &Rig) -> PaneProbe {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        probe_pane(window, rig.pane, "pane").expect("pane")
    })
    .expect("window")
}

fn date_header(cx: &mut TestAppContext, rig: &Rig) -> Box2 {
    probe(cx, rig).header[DATE].expect("date header")
}

fn width_of(bounds: Box2) -> f32 {
    f32::from(bounds.size.width)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.
}

fn date_text_width(window: &Window, row: usize, cx: &gpui_kit::App) -> Pixels {
    let t = tokens::current(cx);
    let mut font = gpui_kit::font(t.typography.numbers_font.clone());
    font.features = t.typography.numbers_font_features.clone();
    font.weight = FontWeight::SEMIBOLD;
    let text = format::date(modified(row));
    let run = TextRun {
        len: text.len(),
        font,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .layout_line(&text, t.typography.small, &[run], None)
        .width
}

fn clipped_dates(cx: &mut TestAppContext, rig: &Rig) -> Vec<String> {
    let pane = rig.pane;
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let probe = probe_pane(window, pane, "pane").expect("pane");
        let mut failures = Vec::new();
        for (row, cells) in &probe.rows {
            let Some(cell) = cells[DATE] else {
                failures.push(format!("row {row} has no Date cell"));
                continue;
            };
            let text = date_text_width(window, *row, cx);
            if text > cell.size.width {
                failures.push(format!(
                    "row {row}: date needs {text:?} but the cell is {:?}",
                    cell.size.width
                ));
            }
            let mut list = window.within(("list", pane.get()));
            let mut cells = list.within(("row", *row));
            let inner = cells.within(("cell", DATE)).find(("cell-text", DATE));
            if outside(cell, inner.bounds()) {
                failures.push(format!("row {row}: the date text leaves its cell"));
            }
        }
        failures
    })
    .expect("window")
}

fn assert_dates_fit(cx: &mut TestAppContext, scale: i64) {
    let rig = open_with(cx, &[setting(Field::UiScale, scale)], 1280.);
    let failures = clipped_dates(cx, &rig);
    assert!(
        failures.is_empty(),
        "at {scale} % ui scale:\n  {}",
        failures.join("\n  ")
    );
}

fn with_bundled_fonts(cx: &TestAppContext) -> TestAppContext {
    let platform = gpui_kit::platform::current_platform(true);
    let fonts =
        TestAppContext::build_with_text_system(cx.dispatcher.clone(), None, platform.text_system());
    fonts.update(|cx| crate::assets::load_fonts(cx).expect("bundled fonts load"));
    fonts
}

#[gpui_kit::test]
fn dates_fit_their_cell_at_ui_scale_100(cx: &mut TestAppContext) {
    assert_dates_fit(cx, 100);
}

#[gpui_kit::test]
fn dates_fit_their_cell_at_ui_scale_150(cx: &mut TestAppContext) {
    assert_dates_fit(cx, 150);
}

#[gpui_kit::test]
#[cfg_attr(windows, ignore = "the headless Windows platform has no text system")]
fn dates_fit_their_cell_with_the_bundled_fonts_at_100_and_150(cx: &mut TestAppContext) {
    for scale in [100, 150] {
        let mut fonts = with_bundled_fonts(cx);
        assert_dates_fit(&mut fonts, scale);
    }
}

fn handle_center(cx: &mut TestAppContext, rig: &Rig, column: usize) -> Point<Pixels> {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("list", rig.pane.get()))
            .find(("column-handle", column))
            .bounds()
            .center()
    })
    .expect("window")
}

fn drag_divider(cx: &mut TestAppContext, rig: &Rig, column: usize, by: f32) {
    let from = handle_center(cx, rig, column);
    let to = point(from.x + px(by), from.y);
    let path = [from, point(from.x + px(by / 2.), from.y), to];
    drag(
        cx,
        rig.window,
        (MouseButton::Left, Modifiers::default()),
        &path,
    );
    dispatch(
        cx,
        rig.window,
        up(MouseButton::Left, to, Modifiers::default()),
    );
}

fn stored(cx: &mut TestAppContext, rig: &Rig, field: Field) -> Value {
    rig.view
        .read_with(cx, |view, _| view.values.get(field).clone())
}

#[gpui_kit::test]
fn dragging_the_date_divider_left_widens_date_and_the_rows_follow(cx: &mut TestAppContext) {
    let rig = open_with(cx, &[], 1280.);
    let before = width_of(date_header(cx, &rig));
    let from = handle_center(cx, &rig, DATE);
    let path = [
        from,
        point(from.x - px(20.), from.y),
        point(from.x - px(40.), from.y),
    ];
    drag(
        cx,
        rig.window,
        (MouseButton::Left, Modifiers::default()),
        &path,
    );
    let during = probe(cx, &rig);
    assert!(near(
        width_of(during.header[DATE].expect("date")),
        before + 40.
    ));
    lined_up(1280., vec![during]);
    let end = point(from.x - px(40.), from.y);
    dispatch(
        cx,
        rig.window,
        up(MouseButton::Left, end, Modifiers::default()),
    );
    let after = probe(cx, &rig);
    let width = width_of(after.header[DATE].expect("date"));
    assert!(near(width, before + 40.), "{before} -> {width}");
    lined_up(1280., vec![after]);
    assert_eq!(
        stored(cx, &rig, Field::WindowDateWidth),
        Value::Number(width.round() as i64)
    );
}

#[gpui_kit::test]
fn a_divider_dragged_far_right_stops_at_the_minimum_width(cx: &mut TestAppContext) {
    let rig = open_with(cx, &[], 1280.);
    drag_divider(cx, &rig, DATE, 600.);
    assert!(near(width_of(date_header(cx, &rig)), 40.));
    let failures = clipped_dates(cx, &rig);
    assert!(!failures.is_empty());
    assert!(
        failures.iter().all(|line| !line.contains("leaves")),
        "{failures:?}"
    );
}

#[gpui_kit::test]
fn a_double_click_on_the_divider_fits_the_visible_dates(cx: &mut TestAppContext) {
    let rig = open_with(cx, &[setting(Field::WindowDateWidth, 260)], 1280.);
    assert!(near(width_of(date_header(cx, &rig)), 260.));
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("list", rig.pane.get()))
            .double_click(("column-handle", DATE), cx);
    })
    .expect("window");
    let fitted = width_of(date_header(cx, &rig));
    assert!(fitted < 200., "{fitted}");
    assert!(clipped_dates(cx, &rig).is_empty());
}

#[gpui_kit::test]
fn a_saved_width_comes_back_at_the_start_and_scales_with_the_ui(cx: &mut TestAppContext) {
    let rig = open_with(cx, &[setting(Field::WindowSizeWidth, 120)], 1280.);
    let size = probe(cx, &rig).header[2].expect("size");
    assert!(near(width_of(size), 120.));
    let scaled = open_with(
        cx,
        &[
            setting(Field::WindowSizeWidth, 120),
            setting(Field::UiScale, 150),
        ],
        1600.,
    );
    let size = probe(cx, &scaled).header[2].expect("size");
    assert!(near(width_of(size), 180.));
}

#[gpui_kit::test]
fn reset_column_widths_brings_the_measured_widths_back(cx: &mut TestAppContext) {
    let rig = open_with(cx, &[], 1280.);
    let before = width_of(date_header(cx, &rig));
    drag_divider(cx, &rig, DATE, -60.);
    assert!(near(width_of(date_header(cx, &rig)), before + 60.));
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ResetColumnWidths), cx);
        window.render_frame(cx);
    })
    .expect("window");
    assert!(near(width_of(date_header(cx, &rig)), before));
    assert!(rig.view.read_with(cx, |view, _| {
        !view.values.is_changed(Field::WindowDateWidth)
    }));
}

#[gpui_kit::test]
fn a_wide_saved_column_still_drops_in_order_on_a_narrow_pane(cx: &mut TestAppContext) {
    let wide = [
        setting(Field::WindowDateWidth, 320),
        setting(Field::WindowSizeWidth, 320),
    ];
    for width in [480., 800.] {
        let rig = open_with(cx, &wide, width);
        let pane = probe(cx, &rig);
        assert!(drop_order_breaks(&pane).is_none(), "at {width}");
        for cell in pane.header.iter().flatten() {
            assert!(
                !outside(pane.card, *cell),
                "at {width} a header cell leaves the pane"
            );
        }
        lined_up(width, vec![pane]);
    }
}
