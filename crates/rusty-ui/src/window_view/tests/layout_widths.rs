use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, Entity, Pixels, TestAppContext, Window, px,
};
use rusty_core::{AppState, Command, Direction, PaneId, apply};
use rusty_fs::FakeFs;

use super::{open_options_sized, options_for};
use crate::window_view::FerriteWindow;

const DEEP: &str = "home/vehicle-calibration-measurements-2026/powertrain-control-unit-variants/endurance-test-campaign-exports/thermal-soak-and-cold-start-logs";
const COLUMNS: [&str; 4] = ["Name", "Ext", "Size", "Date"];
pub(super) const ROWS: usize = 30;
const TABS: usize = 20;
const SLACK: f32 = 1.;

pub(super) type Box2 = Bounds<Pixels>;

pub(super) struct PaneProbe {
    pub name: &'static str,
    pub card: Box2,
    pub header: [Option<Box2>; 4],
    pub rows: Vec<(usize, [Option<Box2>; 4])>,
}

fn x(value: Pixels) -> f32 {
    f32::from(value)
}

fn span(bounds: Box2) -> String {
    format!("{:.0}..{:.0}", x(bounds.left()), x(bounds.right()))
}

fn layout_fs() -> std::sync::Arc<FakeFs> {
    let fs = FakeFs::new();
    for i in 0..ROWS as u64 {
        let name =
            format!("{DEEP}/quarterly-measurement-export-with-a-long-descriptive-name-{i:02}.csv");
        fs.add_file(&name, 4_000_000_000 + i * 12_345_679);
    }
    std::sync::Arc::new(fs)
}

fn layout_state(fs: &FakeFs) -> AppState {
    let mut state = AppState::new(fs.location(DEEP));
    let _ = apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    for _ in 1..TABS {
        let _ = apply(&mut state, &Command::PaneOpenTab { location: None });
    }
    state
}

fn open_at(cx: &mut TestAppContext, width: f32) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = layout_fs();
    let state = layout_state(&fs);
    let (window, view) = open_options_sized(cx, options_for(fs, state), px(width));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    (window, view)
}

pub(super) fn panes(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
) -> (Vec<PaneId>, PaneId) {
    view.read_with(cx, |view, _| {
        (view.panes(), view.browser.state().focused_pane())
    })
}

pub(super) fn pane_name(index: usize) -> &'static str {
    if index == 0 {
        "left pane"
    } else {
        "right pane"
    }
}

fn visible_card(window: &Window, pane: PaneId) -> Option<Box2> {
    window
        .try_find(("pane", pane.get()))
        .filter(|card| card.visible() && card.bounds().size.width > px(0.))
        .map(|card| card.bounds())
}

fn row_cells(window: &mut Window, pane: PaneId, row: usize) -> Option<[Option<Box2>; 4]> {
    let mut list = window.within(("list", pane.get()));
    list.try_find(("row", row)).filter(|r| r.visible())?;
    let cells = list.within(("row", row));
    Some(std::array::from_fn(|k| {
        cells.try_find(("cell", k)).map(|c| c.bounds())
    }))
}

pub(super) fn probe_pane(
    window: &mut Window,
    pane: PaneId,
    name: &'static str,
) -> Option<PaneProbe> {
    let card = visible_card(window, pane)?;
    let header = {
        let list = window.within(("list", pane.get()));
        std::array::from_fn(|k| list.try_find(("sort", k)).map(|c| c.bounds()))
    };
    let rows = (0..ROWS)
        .filter_map(|row| row_cells(window, pane, row).map(|cells| (row, cells)))
        .collect();
    Some(PaneProbe {
        name,
        card,
        header,
        rows,
    })
}

fn expected_panes(width: f32) -> usize {
    if width < 900. { 1 } else { 2 }
}

fn probe_split(cx: &mut TestAppContext, width: f32, ratio: f32, expected: usize) -> Vec<PaneProbe> {
    let (window, view) = open_at(cx, width);
    view.update(cx, |view, cx| {
        view.browser
            .command(&Command::LayoutRatio {
                split: rusty_core::SplitPath(Vec::new()),
                ratio: (ratio * 1000.).round() as u16,
            })
            .expect("ratio");
        cx.notify();
    });
    let (panes, _) = panes(cx, &view);
    let probes: Vec<PaneProbe> = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            panes
                .iter()
                .enumerate()
                .filter_map(|(i, pane)| probe_pane(window, *pane, pane_name(i)))
                .collect()
        })
        .expect("window");
    assert_eq!(
        probes.len(),
        expected,
        "at {width} px the probe found {} panes, not {expected}",
        probes.len()
    );
    for pane in &probes {
        assert!(
            !pane.rows.is_empty(),
            "at {width} px the {} shows no rows",
            pane.name
        );
        assert!(
            pane.header[0].is_some(),
            "at {width} px the {} has no Name header",
            pane.name
        );
    }
    probes
}

fn probe(cx: &mut TestAppContext, width: f32) -> Vec<PaneProbe> {
    probe_split(cx, width, 0.5, expected_panes(width))
}

fn report(width: f32, what: &str, failures: &[String]) {
    assert!(
        failures.is_empty(),
        "at {width} px, {what}:\n  {}",
        failures.join("\n  ")
    );
}

fn mismatch(
    column: &str,
    head: Option<Box2>,
    (row, cell): (usize, Option<Box2>),
) -> Option<String> {
    match (head, cell) {
        (Some(head), Some(cell)) => {
            let left = (x(head.left()) - x(cell.left())).abs();
            let right = (x(head.right()) - x(cell.right())).abs();
            (left > SLACK || right > SLACK).then(|| {
                format!(
                    "{column} column: header {} but row {row} {}",
                    span(head),
                    span(cell)
                )
            })
        }
        (Some(_), None) => Some(format!(
            "{column} column: header shows it, row {row} does not"
        )),
        (None, Some(_)) => Some(format!(
            "{column} column: row {row} shows it, header does not"
        )),
        (None, None) => None,
    }
}

fn columns_line_up(cx: &mut TestAppContext, width: f32) {
    lined_up(width, probe(cx, width));
}

pub(super) fn lined_up(width: f32, probes: Vec<PaneProbe>) {
    let mut failures = Vec::new();
    for pane in probes {
        if pane.rows.is_empty() {
            failures.push(format!("{} shows no rows", pane.name));
        }
        for (k, column) in COLUMNS.iter().enumerate() {
            let wrong: Vec<String> = pane
                .rows
                .iter()
                .filter_map(|(row, cells)| mismatch(column, pane.header[k], (*row, cells[k])))
                .collect();
            if let Some(first) = wrong.first() {
                failures.push(format!(
                    "{} {first} ({} of {} rows)",
                    pane.name,
                    wrong.len(),
                    pane.rows.len()
                ));
            }
        }
    }
    report(width, "header and row columns differ", &failures);
}

pub(super) fn outside(card: Box2, cell: Box2) -> bool {
    x(cell.left()) < x(card.left()) - SLACK || x(cell.right()) > x(card.right()) + SLACK
}

fn cells_stay_inside(cx: &mut TestAppContext, width: f32) {
    let mut failures = Vec::new();
    for pane in probe(cx, width) {
        for (k, column) in COLUMNS.iter().enumerate() {
            if let Some(head) = pane.header[k].filter(|b| outside(pane.card, *b)) {
                failures.push(format!(
                    "{} {column} header cell {} leaves the pane {}",
                    pane.name,
                    span(head),
                    span(pane.card)
                ));
            }
            let first = pane.rows.iter().find_map(|(row, cells)| {
                cells[k]
                    .filter(|b| outside(pane.card, *b))
                    .map(|b| (row, b))
            });
            if let Some((row, cell)) = first {
                failures.push(format!(
                    "{} {column} cell of row {row} {} leaves the pane {}",
                    pane.name,
                    span(cell),
                    span(pane.card)
                ));
            }
        }
    }
    report(width, "cells extend past their pane", &failures);
}

fn shown_panes(cx: &mut TestAppContext, width: f32) -> usize {
    let (window, view) = open_at(cx, width);
    let (panes, _) = panes(cx, &view);
    cx.update_window(window, |_, window, _| {
        panes
            .iter()
            .filter(|pane| visible_card(window, **pane).is_some())
            .count()
    })
    .expect("window")
}

fn one_pane(cx: &mut TestAppContext, width: f32) {
    let shown = shown_panes(cx, width);
    assert_eq!(
        shown, 1,
        "at {width} px the window shows {shown} panes; below 900 px it should show one"
    );
}

fn overflowing(
    window: &mut Window,
    scope: ElementId,
    items: Vec<(String, ElementId)>,
    edge: (&str, Box2),
) -> Vec<String> {
    let scoped = window.within(scope);
    items
        .into_iter()
        .filter_map(|(name, id)| {
            let b = scoped.try_find(id)?.bounds();
            outside(edge.1, b).then(|| {
                format!(
                    "{name} {} overflows the {} {}",
                    span(b),
                    edge.0,
                    span(edge.1)
                )
            })
        })
        .collect()
}

fn bar_failures(
    window: &mut Window,
    pane: PaneId,
    bar_id: ElementId,
    (bar_name, items): (&str, Vec<(String, ElementId)>),
) -> Vec<String> {
    let Some(card) = visible_card(window, pane) else {
        return vec!["the focused pane is not shown".to_owned()];
    };
    let Some(bar) = window.try_find(bar_id.clone()) else {
        return vec![format!("the focused pane has no {bar_name}")];
    };
    let bar = bar.bounds();
    let mut failures = Vec::new();
    if outside(card, bar) {
        failures.push(format!(
            "{bar_name} {} leaves the pane {}",
            span(bar),
            span(card)
        ));
    }
    failures.extend(overflowing(window, bar_id, items, (bar_name, bar)));
    failures
}

fn focused_failures(
    cx: &mut TestAppContext,
    width: f32,
    check: impl FnOnce(&mut Window, PaneId) -> Vec<String>,
) -> Vec<String> {
    let (window, view) = open_at(cx, width);
    let (_, focused) = panes(cx, &view);
    cx.update_window(window, |_, window, _| check(window, focused))
        .expect("window")
}

fn tabs_fit(cx: &mut TestAppContext, width: f32) {
    let failures = focused_failures(cx, width, |window, pane| {
        let tabs = (0..TABS)
            .map(|i| (format!("tab {i}"), ElementId::from(i)))
            .collect();
        let mut failures =
            bar_failures(window, pane, ("tabs", pane.get()).into(), ("tab row", tabs));
        let card = visible_card(window, pane);
        let plus = window
            .within(("pane", pane.get()))
            .try_find("new-tab")
            .map(|plus| plus.bounds());
        match (card, plus) {
            (Some(card), Some(plus)) if outside(card, plus) => failures.push(format!(
                "new-tab button {} leaves the pane {}",
                span(plus),
                span(card)
            )),
            (Some(_), None) => failures.push("new-tab button is missing".to_owned()),
            _ => {}
        }
        failures
    });
    report(width, &format!("{TABS} tabs overflow"), &failures);
}

fn missing_crumbs(window: &mut Window, pane: PaneId, needs_menu: bool) -> Vec<String> {
    let last = crate::pane_chrome::path_segments(&FakeFs::new().location(DEEP)).len() - 1;
    let path = window.within(("path", pane.get()));
    let mut required = vec![("root crumb", ElementId::from(("segment", 0usize)))];
    required.push(("last crumb", ElementId::from(("segment", last))));
    if needs_menu {
        required.push(("\u{2026} crumb", ElementId::from("path-more")));
    }
    required
        .into_iter()
        .filter(|(_, id)| path.try_find(id.clone()).is_none())
        .map(|(name, _)| format!("the path bar has no {name}"))
        .collect()
}

fn path_fits(cx: &mut TestAppContext, width: f32) {
    let failures = focused_failures(cx, width, |window, pane| {
        let segments = (0..16usize)
            .map(|i| (format!("path segment {i}"), ElementId::from(("segment", i))))
            .collect();
        let mut failures = bar_failures(
            window,
            pane,
            ("path", pane.get()).into(),
            ("path bar", segments),
        );
        failures.extend(missing_crumbs(window, pane, width < 900.));
        failures
    });
    report(width, "the path bar overflows", &failures);
}

#[gpui_kit::test]
fn columns_line_up_at_480(cx: &mut TestAppContext) {
    columns_line_up(cx, 480.);
}

#[gpui_kit::test]
fn columns_line_up_at_800(cx: &mut TestAppContext) {
    columns_line_up(cx, 800.);
}

#[gpui_kit::test]
fn columns_line_up_at_1280(cx: &mut TestAppContext) {
    columns_line_up(cx, 1280.);
}

#[gpui_kit::test]
fn cells_stay_inside_their_pane_at_480(cx: &mut TestAppContext) {
    cells_stay_inside(cx, 480.);
}

#[gpui_kit::test]
fn cells_stay_inside_their_pane_at_800(cx: &mut TestAppContext) {
    cells_stay_inside(cx, 800.);
}

#[gpui_kit::test]
fn cells_stay_inside_their_pane_at_1280(cx: &mut TestAppContext) {
    cells_stay_inside(cx, 1280.);
}

#[gpui_kit::test]
fn a_480_px_window_shows_one_pane(cx: &mut TestAppContext) {
    one_pane(cx, 480.);
}

#[gpui_kit::test]
fn an_800_px_window_shows_one_pane(cx: &mut TestAppContext) {
    one_pane(cx, 800.);
}

#[gpui_kit::test]
fn a_1280_px_window_shows_both_panes(cx: &mut TestAppContext) {
    let shown = shown_panes(cx, 1280.);
    assert_eq!(
        shown, 2,
        "at 1280 px the window shows {shown} panes, not two"
    );
}

pub(super) fn drop_order_breaks(pane: &PaneProbe) -> Option<String> {
    let [_, ext, size, date] = pane.header.map(|cell| cell.is_some());
    let broken = (ext && !(date && size)) || (date && !size);
    broken.then(|| format!("{} shows Ext {ext}, Date {date}, Size {size}", pane.name))
}

#[gpui_kit::test]
fn columns_drop_ext_first_then_date_then_size(cx: &mut TestAppContext) {
    let mut failures = Vec::new();
    for width in [480., 800., 900., 1280.] {
        let broken = probe(cx, width)
            .iter()
            .filter_map(drop_order_breaks)
            .collect::<Vec<_>>();
        failures.extend(
            broken
                .into_iter()
                .map(|line| format!("at {width} px the {line}")),
        );
    }
    assert!(
        failures.is_empty(),
        "columns drop out of order:\n  {}",
        failures.join("\n  ")
    );
}

#[gpui_kit::test]
fn twenty_tabs_fit_at_480(cx: &mut TestAppContext) {
    tabs_fit(cx, 480.);
}

#[gpui_kit::test]
fn twenty_tabs_fit_at_800(cx: &mut TestAppContext) {
    tabs_fit(cx, 800.);
}

#[gpui_kit::test]
fn twenty_tabs_fit_at_1280(cx: &mut TestAppContext) {
    tabs_fit(cx, 1280.);
}

#[gpui_kit::test]
fn the_path_bar_fits_at_480(cx: &mut TestAppContext) {
    path_fits(cx, 480.);
}

#[gpui_kit::test]
fn the_path_bar_fits_at_800(cx: &mut TestAppContext) {
    path_fits(cx, 800.);
}

#[gpui_kit::test]
fn the_path_bar_fits_at_1280(cx: &mut TestAppContext) {
    path_fits(cx, 1280.);
}

fn shown_columns(pane: &PaneProbe) -> [bool; 3] {
    let [_, ext, size, date] = pane.header.map(|cell| cell.is_some());
    [ext, date, size]
}

#[gpui_kit::test]
fn a_narrowed_pane_drops_date_and_keeps_size(cx: &mut TestAppContext) {
    let probes = probe_split(cx, 1000., 0.35, 2);
    assert_eq!(
        shown_columns(&probes[0]),
        [false, false, true],
        "left pane {:?}",
        probes[0].card.size.width
    );
    assert!(drop_order_breaks(&probes[1]).is_none());
    lined_up(1000., probes);
}

#[gpui_kit::test]
fn an_undersized_layout_restores_full_width_columns(cx: &mut TestAppContext) {
    let probes = probe_split(cx, 1000., 0.22, 1);
    assert_eq!(
        shown_columns(&probes[0]),
        [true, true, true],
        "shown pane {:?}",
        probes[0].card.size.width
    );
    lined_up(1000., probes);
}
