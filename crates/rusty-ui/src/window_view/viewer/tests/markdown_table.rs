use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, FontWeight, Pixels, ScrollDelta,
    TestAppContext, TextRun, font, point, px,
};

use super::structured::open_named;
use super::{Setup, has_editor, redraw};
use crate::tokens;

const TIERS: &str = "| Tier | Model | Roles | Used for |\n| --- | --- | --- | --- |\n\
    | **T1** | Opus 5.5, `medium` | `patcher`, `infra-builder` | Moves, exact edits, small \
    config, scaffolds |\n\
    | **T4** | Opus 5.5, `high`: the orchestrator itself, or `ui-builder-high` for hard UI | \
    orchestrator, `ui-builder-high` | The core and the hardest atoms: every `rusty-core` Green \
    atom, everything that can lose user data (`atomic_write`, settings, session, job journal, \
    job engine, standard copy), the spike, `FileList`, the pane view |\n";

fn word_width(cx: &mut TestAppContext, window: AnyWindowHandle, word: &str, mono: bool) -> Pixels {
    cx.update_window(window, |_, window, cx| {
        let t = tokens::current(cx).clone();
        let mut face = font(if mono {
            t.typography.mono_font.clone()
        } else {
            t.typography.ui_font.clone()
        });
        face.weight = FontWeight::BOLD;
        let run = TextRun {
            len: word.len(),
            font: face,
            color: t.colors.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .layout_line(word, t.typography.small, &[run], None);
        line.width + t.spacing.md * 2.
    })
    .expect("window")
}

fn bounds_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Bounds<Pixels> {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find(id).bounds()
    })
    .expect("window")
}

fn cell(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    block: usize,
    column: usize,
) -> Bounds<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("md-block", block))
            .find(("md-col", column))
            .bounds()
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_short_column_keeps_tier_and_t1_on_one_line(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "tiers.md", Setup::new(TIERS.as_bytes().to_vec()));
    assert!(!has_editor(cx, &view));
    let tier = word_width(cx, window, "Tier", false);
    for block in 0..3 {
        let width = cell(cx, window, block, 0).size.width;
        assert!(width >= tier, "row {block}: {width:?} < {tier:?}");
    }
    let line = cx.update(|cx| tokens::current(cx).typography.line_height);
    let head = cell(cx, window, 0, 0).size.height;
    assert!(head < line * 2., "the head row is one line: {head:?}");
}

#[gpui_kit::test]
fn every_column_is_at_least_as_wide_as_its_longest_word(cx: &mut TestAppContext) {
    let (window, _, _) = open_named(cx, "tiers.md", Setup::new(TIERS.as_bytes().to_vec()));
    let model = word_width(cx, window, "ui-builder-high", true);
    for block in 0..3 {
        let width = cell(cx, window, block, 1).size.width;
        assert!(width >= model, "row {block}: {width:?} < {model:?}");
    }
    let short = cell(cx, window, 2, 0).size.width;
    let long = cell(cx, window, 2, 3).size.width;
    assert!(
        long > short * 3.,
        "spare width follows content: {short:?} {long:?}"
    );
}

fn wide_table() -> String {
    let word = "w".repeat(30);
    let head = format!("|{}\n", format!(" {word} |").repeat(12));
    let rule = format!("|{}\n", " --- |".repeat(12));
    let row = format!("|{}\n", " x |".repeat(12));
    format!("{head}{rule}{row}{row}\nAfter the table.\n")
}

#[gpui_kit::test]
fn a_table_wider_than_the_view_scrolls_sideways_inside_its_block(cx: &mut TestAppContext) {
    let (window, _, _) = open_named(cx, "wide.md", Setup::new(wide_table().into_bytes()));
    let body = bounds_of(cx, window, "markdown-body");
    let frame = bounds_of(cx, window, ("md-table", 1usize));
    let row = bounds_of(cx, window, ("md-row", 1usize));
    assert!(frame.size.width <= body.size.width, "{frame:?} in {body:?}");
    assert!(row.size.width > frame.size.width, "{row:?} in {frame:?}");
    let after = bounds_of(cx, window, ("md-block", 3usize));
    assert!(after.size.width <= body.size.width, "{after:?} in {body:?}");
    cx.update_window(window, |_, window, cx| {
        window.scroll(
            ("md-table", 1usize),
            ScrollDelta::Pixels(point(px(-200.), px(0.))),
            cx,
        );
    })
    .expect("window");
    redraw(cx, window);
    let moved = bounds_of(cx, window, ("md-row", 1usize));
    let head = bounds_of(cx, window, ("md-row", 0usize));
    assert!(moved.origin.x < row.origin.x, "{moved:?} after {row:?}");
    assert_eq!(head.origin.x, moved.origin.x, "the rows move together");
    assert_eq!(bounds_of(cx, window, "markdown-body"), body);
    assert_eq!(
        bounds_of(cx, window, ("md-block", 3usize)).origin,
        after.origin
    );
}

#[gpui_kit::test]
fn a_wide_table_shows_a_horizontal_scrollbar_under_its_last_row(cx: &mut TestAppContext) {
    let (window, _, _) = open_named(cx, "wide.md", Setup::new(wide_table().into_bytes()));
    let bar = bounds_of(cx, window, ("md-table-bar", 0usize));
    let last = bounds_of(cx, window, ("md-table", 2usize));
    assert!(bar.size.width > px(0.));
    assert!(bar.origin.y >= last.origin.y, "{bar:?} under {last:?}");
}
