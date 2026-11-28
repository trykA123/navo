mod sessions;

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Context, Entity, Render, TestAppContext, Window, div, px};
use navo_term::grid::{
    Cell, Color, Cursor, CursorShape, Grid, Point, RowCells, Selection, Snapshot, Viewport, Width,
};
use navo_term::{Sink, Size};

use super::*;
use crate::prelude::*;
use crate::test_window;
use crate::tokens::{self, Mode, TerminalColors, TerminalScheme, Theme, contrast_ratio};

fn cell(c: char) -> Cell {
    Cell {
        c,
        ..Cell::default()
    }
}

fn text_row(text: &str, cols: usize) -> RowCells {
    let mut row: Vec<Cell> = text.chars().take(cols).map(cell).collect();
    row.resize(cols, Cell::default());
    row
}

fn snapshot_with_rows(lines: &[&str], cols: usize, damaged: Vec<u16>) -> Snapshot {
    Snapshot {
        rows: lines.iter().map(|line| text_row(line, cols)).collect(),
        cursor: Some(Cursor {
            row: 0,
            col: 0,
            shape: CursorShape::Block,
        }),
        selection: None,
        display_offset: 0,
        damaged,
    }
}

struct GridHarness {
    snapshot: Arc<Snapshot>,
    cache: Entity<RowCache>,
    style: GridStyle,
}

impl Render for GridHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            TerminalGrid::new(Arc::clone(&self.snapshot), self.cache.clone())
                .style(self.style.clone()),
        )
    }
}

fn open_harness(
    cx: &mut TestAppContext,
    width: Pixels,
    snapshot: Snapshot,
    style: GridStyle,
) -> (AnyWindowHandle, Entity<GridHarness>, Entity<RowCache>) {
    let mut cache_out = None;
    let (window, view) = test_window::open_sized(cx, width, |_, cx| {
        let cache = cx.new(|_| RowCache::new());
        cache_out = Some(cache.clone());
        GridHarness {
            snapshot: Arc::new(snapshot),
            cache,
            style,
        }
    });
    let cache = cache_out.expect("harness creates cache");
    (window, view, cache)
}

#[test]
fn grid_dimensions_compute_exact_and_truncated_counts() {
    assert_eq!(
        grid_dimensions(px(800.0), px(360.0), px(8.0), px(18.0)),
        (100, 20)
    );
    assert_eq!(
        grid_dimensions(px(807.9), px(377.9), px(8.0), px(18.0)),
        (100, 20)
    );
    assert_eq!(
        grid_dimensions(px(0.0), px(360.0), px(8.0), px(18.0)),
        (0, 20)
    );
    assert_eq!(
        grid_dimensions(px(800.0), px(360.0), px(0.0), px(18.0)),
        (0, 0)
    );
    assert_eq!(
        grid_dimensions(px(-10.0), px(360.0), px(8.0), px(18.0)),
        (0, 20)
    );
}

#[test]
fn ligatures_are_disabled_for_terminal_cells() {
    let features = terminal_font_features();
    let tags = features.tag_value_list();
    assert!(
        tags.iter().any(|(tag, val)| tag == "calt" && *val == 0),
        "calt must be disabled: {tags:?}"
    );
    assert!(
        tags.iter().any(|(tag, val)| tag == "liga" && *val == 0),
        "liga must be disabled: {tags:?}"
    );
}

#[test]
fn background_run_coalescing_merges_adjacent_and_splits_on_change() {
    let theme = Theme::new(Mode::Dark);
    let colors = &theme.terminal;
    let mut row = vec![Cell::default(); 8];
    row[1].bg = Color::Ansi(1);
    row[2].bg = Color::Ansi(1);
    row[3].bg = Color::Ansi(4);
    row[5].inverse = true;
    row[6].bg = Color::Rgb(10, 20, 30);
    row[7].bg = Color::Rgb(10, 20, 30);

    let runs = coalesce_bg_runs(&row, colors);
    assert_eq!(
        runs,
        vec![
            BgRun {
                start_col: 1,
                len_cols: 2,
                color: colors.ansi_color(1),
            },
            BgRun {
                start_col: 3,
                len_cols: 1,
                color: colors.ansi_color(4),
            },
            BgRun {
                start_col: 5,
                len_cols: 1,
                color: colors.text,
            },
            BgRun {
                start_col: 6,
                len_cols: 2,
                color: tokens::TerminalColors::rgb_color(10, 20, 30),
            },
        ]
    );
}

#[test]
fn selection_columns_handles_stream_block_and_scrollback() {
    let stream = Selection {
        start: Point { line: 1, col: 4 },
        end: Point { line: 3, col: 7 },
        block: false,
    };
    assert_eq!(selection_columns(stream, 0, 20, 0), None);
    assert_eq!(selection_columns(stream, 1, 20, 0), Some(4..20));
    assert_eq!(selection_columns(stream, 2, 20, 0), Some(0..20));
    assert_eq!(selection_columns(stream, 3, 20, 0), Some(0..8));
    assert_eq!(selection_columns(stream, 4, 20, 0), None);

    let block = Selection {
        start: Point { line: -2, col: 3 },
        end: Point { line: 0, col: 6 },
        block: true,
    };
    // With display_offset = 2, viewport row 0 maps to grid line -2.
    assert_eq!(selection_columns(block, 0, 20, 2), Some(3..7));
    assert_eq!(selection_columns(block, 2, 20, 2), Some(3..7));
    assert_eq!(selection_columns(block, 3, 20, 2), None);
}

#[test]
fn cursor_blink_steps_at_half_period_and_stays_visible_under_reduced_motion() {
    assert!(cursor_visible(Duration::ZERO, true, false));
    assert!(cursor_visible(Duration::from_millis(529), true, false));
    assert!(!cursor_visible(Duration::from_millis(530), true, false));
    assert!(!cursor_visible(Duration::from_millis(1059), true, false));
    assert!(cursor_visible(Duration::from_millis(1060), true, false));

    // Reduced motion strictly disables blinking.
    assert!(cursor_visible(Duration::from_millis(600), true, true));
    // Explicit blink off also stays visible.
    assert!(cursor_visible(Duration::from_millis(600), false, false));
}

#[gpui_kit::test]
fn headless_grid_computes_columns_and_rows_from_viewport_and_font(cx: &mut TestAppContext) {
    let style = GridStyle {
        padding: px(8.0),
        ..GridStyle::default()
    };
    let snap = snapshot_with_rows(&["hello", "world"], 40, vec![0, 1]);
    let (window, _, cache) = open_harness(cx, px(800.0), snap, style.clone());

    let expected = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let t = tokens::current(cx);
            let metrics = measure_cell(
                window,
                &t.typography.mono_font,
                style.font_size,
                style.line_height,
            );
            assert!(metrics.width > px(0.0), "cell width must be positive");
            assert!(metrics.height > px(0.0), "cell height must be positive");
            grid_dimensions(
                px(800.0) - style.padding * 2.0,
                px(600.0) - style.padding * 2.0,
                metrics.width,
                metrics.height,
            )
        })
        .expect("window");

    let actual = cx.update(|cx| cache.read(cx).last_grid_dims());
    assert_eq!(actual, expected);
    assert!(
        actual.0 > 20 && actual.1 > 10,
        "unexpected dims: {actual:?}"
    );
}

#[gpui_kit::test]
fn damaged_row_repaint_reshapes_only_the_damaged_row(cx: &mut TestAppContext) {
    let initial = snapshot_with_rows(
        &[
            "row 0: prompt",
            "row 1: ok",
            "row 2: old output",
            "row 3: footer",
            "row 4: ready",
        ],
        40,
        vec![0, 1, 2, 3, 4],
    );
    let (window, view, cache) = open_harness(cx, px(800.0), initial, GridStyle::default());

    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("first paint");
    let after_initial = cx.update(|cx| cache.read(cx).reshape_count());
    assert_eq!(after_initial, 5, "initial paint must shape all 5 rows");

    // Repainting the exact same Arc<Snapshot> (e.g. cursor blink) must shape 0 rows.
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("second paint");
    let after_same = cx.update(|cx| cache.read(cx).reshape_count());
    assert_eq!(after_same, 5, "identical snapshot must reshape 0 rows");

    // Mark ONLY row 2 damaged and repaint: reshape delta must be strictly 1.
    let updated = snapshot_with_rows(
        &[
            "row 0: prompt",
            "row 1: ok",
            "row 2: UPDATED output",
            "row 3: footer",
            "row 4: ready",
        ],
        40,
        vec![2],
    );
    cx.update(|cx| {
        view.update(cx, |harness, cx| {
            harness.snapshot = Arc::new(updated);
            cx.notify();
        });
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("damaged paint");
    let after_one_damaged = cx.update(|cx| cache.read(cx).reshape_count());
    assert_eq!(
        after_one_damaged - after_same,
        1,
        "only the single damaged row may be reshaped"
    );

    // Switching theme invalidates the whole cache and reshapes all 5 rows.
    cx.update(|cx| {
        tokens::apply(Mode::Dark, cx);
        view.update(cx, |_, cx| cx.notify());
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("dark theme paint");
    let after_theme = cx.update(|cx| cache.read(cx).reshape_count());
    assert_eq!(
        after_theme - after_one_damaged,
        5,
        "theme change must invalidate all 5 rows"
    );
}

#[gpui_kit::test]
fn wide_chars_from_real_grid_occupy_two_columns_without_overlap(cx: &mut TestAppContext) {
    let grid = Grid::new(Size { cols: 40, rows: 4 }, 100, || {});
    grid.output("日本語 🚀 != ->\r\n".as_bytes());
    let snap = grid.snapshot(Viewport::default());
    assert_eq!(snap.rows[0][0].width, Width::Wide);
    assert_eq!(snap.rows[0][1].width, Width::Spacer);

    let (window, _, cache) = open_harness(cx, px(800.0), snap, GridStyle::default());
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("wide paint");

    let wide_cols = cx.update(|cx| cache.read(cx).wide_glyph_cols(0));
    // '日' at 0, '本' at 2, '語' at 4, space at 6, '🚀' at 7 (taking 7..9).
    assert_eq!(wide_cols, vec![0, 2, 4, 7]);
}

#[gpui_kit::test]
fn headless_cursor_respects_reduced_motion_and_shapes(cx: &mut TestAppContext) {
    let snap = snapshot_with_rows(&["abc"], 20, vec![0]);
    let style = GridStyle {
        blink_enabled: true,
        blink_elapsed: Duration::from_millis(600),
        ..GridStyle::default()
    };
    let (window, view, cache) = open_harness(cx, px(800.0), snap, style);

    // Motion enabled at 600ms (past 530ms half-period) => cursor blinked off.
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("blink off paint");
    assert_eq!(
        cx.update(|cx| cache.read(cx).last_cursor_visible()),
        Some(false)
    );

    // Under reduced motion at the exact same 600ms => cursor strictly stays visible!
    cx.update(|cx| {
        cx.set_reduce_motion(true);
        view.update(cx, |_, cx| cx.notify());
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("reduced motion paint");
    assert_eq!(
        cx.update(|cx| cache.read(cx).last_cursor_visible()),
        Some(true)
    );
    let block_bounds = cx
        .update(|cx| cache.read(cx).last_cursor_bounds())
        .expect("block cursor bounds");

    // Bar cursor is narrow (<= 3px) and full cell height; Underline cursor is full cell width and thin.
    cx.update(|cx| {
        view.update(cx, |harness, cx| {
            harness.style.cursor_shape = Some(CursorShape::Bar);
            cx.notify();
        });
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("bar cursor paint");
    let bar_bounds = cx
        .update(|cx| cache.read(cx).last_cursor_bounds())
        .expect("bar cursor bounds");
    assert!(bar_bounds.size.width < block_bounds.size.width);
    assert_eq!(bar_bounds.size.height, block_bounds.size.height);

    cx.update(|cx| {
        view.update(cx, |harness, cx| {
            harness.style.cursor_shape = Some(CursorShape::Underline);
            cx.notify();
        });
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("underline cursor paint");
    let underline_bounds = cx
        .update(|cx| cache.read(cx).last_cursor_bounds())
        .expect("underline cursor bounds");
    assert_eq!(underline_bounds.size.width, block_bounds.size.width);
    assert!(underline_bounds.size.height < block_bounds.size.height);
}

#[test]
fn selected_inverse_video_meets_aa_contrast_in_light_and_dark_themes() {
    let inverse = Cell {
        c: 'I',
        inverse: true,
        ..Cell::default()
    };
    for mode in [Mode::Light, Mode::Dark] {
        let theme = Theme::new(mode);
        let colors = &theme.terminal;
        let fg = resolve_selected_fg(&inverse, colors);
        let ratio = contrast_ratio(fg, colors.selection);
        assert!(
            ratio >= 4.5,
            "selected inverse fg in {mode:?} must meet 4.5:1 AA against selection, got {ratio:.2}"
        );
    }
}

#[test]
fn dim_inverse_cell_preserves_dim_attribute() {
    let plain = Cell {
        c: 'A',
        inverse: true,
        ..Cell::default()
    };
    let dimmed = Cell {
        c: 'A',
        inverse: true,
        dim: true,
        ..Cell::default()
    };
    for mode in [Mode::Light, Mode::Dark] {
        let colors = &Theme::new(mode).terminal;
        let plain_colors = resolve_cell_colors(&plain, colors);
        let dimmed_colors = resolve_cell_colors(&dimmed, colors);
        assert_ne!(
            dimmed_colors.fg, plain_colors.fg,
            "SGR 2;7 must dim the resolved foreground relative to plain SGR 7 in {mode:?}"
        );
        assert_eq!(dimmed_colors.bg, plain_colors.bg);
    }
}

#[test]
fn block_cursor_glyph_meets_aa_contrast_across_all_schemes() {
    for mode in [Mode::Light, Mode::Dark] {
        let theme = Theme::new(mode);
        for scheme in TerminalScheme::ALL {
            for keep_readable in [true, false] {
                let colors =
                    TerminalColors::from_scheme(scheme, mode, keep_readable, &theme.colors);
                let fg = block_cursor_glyph_color(&colors);
                let ratio = contrast_ratio(fg, colors.cursor);
                assert!(
                    ratio >= 4.5,
                    "block cursor glyph for {scheme:?} ({mode:?}, readable={keep_readable}) must meet 4.5:1 AA, got {ratio:.2}"
                );
            }
        }
        let palette = tokens::SystemPalette {
            window: theme.colors.background,
            window_text: theme.colors.text,
            highlight: theme.colors.accent,
            highlight_text: theme.colors.background,
            button_face: theme.colors.background,
            button_text: theme.colors.text,
            gray_text: theme.colors.muted,
            hot_light: theme.colors.accent,
        };
        let hc = tokens::contrast_theme(&theme, &palette);
        let fg = block_cursor_glyph_color(&hc.terminal);
        let ratio = contrast_ratio(fg, hc.terminal.cursor);
        assert!(
            ratio >= 4.5,
            "block cursor glyph for High Contrast ({mode:?}) must meet 4.5:1 AA, got {ratio:.2}"
        );
    }
}

#[gpui_kit::test]
fn selection_change_invalidates_only_affected_rows(cx: &mut TestAppContext) {
    let initial = snapshot_with_rows(
        &[
            "row 0: prompt",
            "row 1: ok",
            "row 2: inverse text",
            "row 3: footer",
            "row 4: ready",
        ],
        40,
        vec![0, 1, 2, 3, 4],
    );
    let (window, view, cache) = open_harness(cx, px(800.0), initial, GridStyle::default());

    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("initial paint");
    let after_initial = cx.update(|cx| cache.read(cx).reshape_count());
    assert_eq!(after_initial, 5);

    // Add a selection on row 2 with NO damaged rows: only row 2 should reshape.
    let mut with_sel = snapshot_with_rows(
        &[
            "row 0: prompt",
            "row 1: ok",
            "row 2: inverse text",
            "row 3: footer",
            "row 4: ready",
        ],
        40,
        vec![],
    );
    with_sel.selection = Some(Selection {
        start: Point { line: 2, col: 0 },
        end: Point { line: 2, col: 10 },
        block: false,
    });
    cx.update(|cx| {
        view.update(cx, |harness, cx| {
            harness.snapshot = Arc::new(with_sel);
            cx.notify();
        });
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("selected paint");
    let after_select = cx.update(|cx| cache.read(cx).reshape_count());
    assert_eq!(
        after_select - after_initial,
        1,
        "adding selection on 1 row must reshape exactly 1 row"
    );

    // Clearing selection on row 2 with NO damaged rows: only row 2 should reshape back.
    let cleared = snapshot_with_rows(
        &[
            "row 0: prompt",
            "row 1: ok",
            "row 2: inverse text",
            "row 3: footer",
            "row 4: ready",
        ],
        40,
        vec![],
    );
    cx.update(|cx| {
        view.update(cx, |harness, cx| {
            harness.snapshot = Arc::new(cleared);
            cx.notify();
        });
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("cleared selection paint");
    let after_clear = cx.update(|cx| cache.read(cx).reshape_count());
    assert_eq!(
        after_clear - after_select,
        1,
        "clearing selection on 1 row must reshape exactly 1 row"
    );
}
