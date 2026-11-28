//! Fixed terminal showcase buffer for `navo-ui-gallery`: ANSI 16 fg/bg runs, box drawing,
//! CJK + emoji wide glyphs, ligatures off, selection, and a failed command block in both themes.

use std::sync::Arc;

#[cfg(test)]
use gpui_kit::TestSupportExt;
use navo_term::grid::{Grid, Point as GridPoint, Selection, Snapshot, Viewport};
use navo_term::{Sink, Size};
use navo_ui::prelude::*;
use navo_ui::terminal::{BlockKind, GridStyle, RowBlock, RowCache, TerminalGrid, measure_cell};
use navo_ui::tokens::{self, GUTTER, Theme};

const GRID_COLS: u16 = 84;
const GRID_ROWS: u16 = 22;
const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 1.4;
const GRID_PADDING: f32 = 8.0;
const FAILED_START_ROW: u16 = 12;
const FAILED_END_ROW: u16 = 17;

#[derive(Clone, Copy)]
enum MarkTone {
    Ok,
    Fail,
    Run,
}

#[derive(Clone, Copy)]
struct GutterMark {
    row: u16,
    glyph: &'static str,
    tone: MarkTone,
}

const MARKS: [GutterMark; 4] = [
    GutterMark {
        row: 0,
        glyph: "•",
        tone: MarkTone::Ok,
    },
    GutterMark {
        row: 4,
        glyph: "•",
        tone: MarkTone::Ok,
    },
    GutterMark {
        row: FAILED_START_ROW,
        glyph: "×",
        tone: MarkTone::Fail,
    },
    GutterMark {
        row: 19,
        glyph: "›",
        tone: MarkTone::Run,
    },
];

/// Showcase page demonstrating the GPUI terminal grid element in light and dark themes.
pub struct TerminalPage {
    snapshot: Arc<Snapshot>,
    cache: Entity<RowCache>,
    blocks: Vec<RowBlock>,
}

impl TerminalPage {
    /// Builds the fixed showcase buffer through a real `navo_term::grid::Grid`.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut snapshot = build_showcase_snapshot();
        snapshot.selection = Some(Selection {
            start: GridPoint { line: 16, col: 5 },
            end: GridPoint { line: 16, col: 27 },
            block: false,
        });
        let blocks = vec![RowBlock {
            start_row: FAILED_START_ROW,
            end_row: FAILED_END_ROW,
            kind: BlockKind::Failed,
            current: true,
        }];
        Self {
            snapshot: Arc::new(snapshot),
            cache: cx.new(|_| RowCache::new()),
            blocks,
        }
    }
}

fn build_showcase_snapshot() -> Snapshot {
    let grid = Grid::new(
        Size {
            cols: GRID_COLS,
            rows: GRID_ROWS,
        },
        128,
        || {},
    );
    let stream = concat!(
        // Row 0: Prompt + operators with ligatures off
        "\x1b[1;34m~/navo\x1b[0m $ echo \"ANSI 16 · != -> >= ===\"\r\n",
        // Row 1: ANSI 16 foreground colors (0..=15)
        "\x1b[30m0:blk \x1b[31m1:red \x1b[32m2:grn \x1b[33m3:ylw \x1b[34m4:blu \x1b[35m5:mag \x1b[36m6:cyn \x1b[37m7:white\x1b[0m ",
        "\x1b[90m8:gry \x1b[91m9:RED \x1b[92m10:GRN \x1b[93m11:YLW \x1b[94m12:BLU \x1b[95m13:MAG \x1b[96m14:CYN \x1b[97m15:WHITE\x1b[0m\r\n",
        // Row 2: ANSI background runs (coalesced horizontal quads)
        "BG: ",
        "\x1b[41;37m  RED  \x1b[42;30m  GRN  \x1b[43;30m  YLW  \x1b[44;37m  BLU  \x1b[45;37m  MAG  \x1b[46;30m  CYN  \x1b[0m ",
        "\x1b[7m INVERSE \x1b[0m \x1b[2mdim\x1b[0m \x1b[3mitalic\x1b[0m \x1b[4munderline\x1b[0m \x1b[9mstrike\x1b[0m\r\n",
        // Row 3: Blank line
        "\r\n",
        // Row 4: Box drawing & wide characters prompt
        "\x1b[1;34m~/navo\x1b[0m $ navo-term --glyph-check\r\n",
        // Rows 5..=10: Box drawing table with CJK + emoji wide chars
        "┌────────────────────────┬────────────────────────┬────────────────────┐\r\n",
        "│ \x1b[1mScript / Feature\x1b[0m       │ \x1b[1mSample\x1b[0m                 │ \x1b[1mAlignment\x1b[0m          │\r\n",
        "├────────────────────────┼────────────────────────┼────────────────────┤\r\n",
        "│ CJK wide (2 cols)      │ 日本語 · 中文 · 한국어 │ \x1b[32mok\x1b[0m                 │\r\n",
        "│ Emoji + operators      │ 🚀 ✅ 🔧  != -> >=     │ \x1b[32mok\x1b[0m                 │\r\n",
        "└────────────────────────┴────────────────────────┴────────────────────┘\r\n",
        // Row 11: Blank line
        "\r\n",
        // Rows 12..=17: Failed command block
        "\x1b[1;34m~/navo\x1b[0m $ cargo test -p navo-term\r\n",
        "\x1b[2mrunning 3 tests\x1b[0m\r\n",
        "test grid::damage_single_row ... \x1b[32mok\x1b[0m\r\n",
        "test prompt::osc_133_nonce ... \x1b[32mok\x1b[0m\r\n",
        "test pty::conpty_close_drain ... \x1b[1;31mFAILED\x1b[0m\r\n",
        "\x1b[1;31merror:\x1b[0m test failed, to rerun pass '-p navo-term'\r\n",
        // Row 18: Blank line
        "\r\n",
        // Row 19: Idle prompt with visible cursor
        "\x1b[1;34m~/navo\x1b[0m $ ",
    );
    grid.output(stream.as_bytes());
    grid.snapshot(Viewport::default())
}

fn gutter(t: &Theme, cell_height: Pixels, total_height: Pixels) -> Div {
    let mut column = div()
        .relative()
        .flex_none()
        .w(px(GUTTER))
        .h(total_height)
        .bg(t.terminal.background);
    for mark in MARKS {
        let color = match mark.tone {
            MarkTone::Ok => t.terminal.mark_ok,
            MarkTone::Fail => t.terminal.mark_fail,
            MarkTone::Run => t.terminal.mark_run,
        };
        let top = px(GRID_PADDING) + cell_height * f32::from(mark.row);
        column = column.child(
            div()
                .absolute()
                .top(top)
                .left_0()
                .w(px(GUTTER))
                .h(cell_height)
                .flex()
                .items_center()
                .justify_center()
                .text_color(color)
                .text_size(t.typography.body)
                .child(mark.glyph),
        );
    }
    column
}

fn failed_chip(t: &Theme, cell_height: Pixels) -> Div {
    let top = px(GRID_PADDING) + cell_height * f32::from(FAILED_START_ROW);
    div()
        .absolute()
        .top(top)
        .right(t.spacing.lg)
        .px(t.spacing.sm)
        .py(t.spacing.xxs)
        .rounded(t.radii.sm)
        .border_1()
        .border_color(t.terminal.mark_fail)
        .bg(t.terminal.chrome)
        .text_color(t.terminal.mark_fail)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.tiny)
        .child("exit 101")
}

impl Render for TerminalPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let metrics = measure_cell(window, &t.typography.mono_font, px(FONT_SIZE), LINE_HEIGHT);
        let grid_height = metrics.height * f32::from(GRID_ROWS) + px(GRID_PADDING * 2.0);
        let style = GridStyle {
            font_family: Some(t.typography.mono_font.clone()),
            font_size: px(FONT_SIZE),
            line_height: LINE_HEIGHT,
            padding: px(GRID_PADDING),
            colors: Some(t.terminal),
            blink_enabled: false,
            ..GridStyle::default()
        };
        let grid = TerminalGrid::new(Arc::clone(&self.snapshot), self.cache.clone())
            .style(style)
            .blocks(self.blocks.clone())
            .height(grid_height);
        let card = div()
            .id("terminal-showcase")
            .relative()
            .flex()
            .w_full()
            .max_w(px(920.0))
            .rounded(t.radii.card)
            .border_1()
            .border_color(t.colors.line)
            .bg(t.terminal.background)
            .overflow_hidden()
            .child(gutter(&t, metrics.height, grid_height))
            .child(div().flex_1().min_w_0().child(grid))
            .child(failed_chip(&t, metrics.height));
        #[cfg(test)]
        let card = card.test_support();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(t.spacing.lg)
            .p(t.spacing.xxl)
            .bg(t.colors.background)
            .child(
                div()
                    .text_size(t.typography.small)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.muted)
                    .child("TERMINAL GRID — ANSI 16, BOX DRAWING, CJK + EMOJI, FAILED BLOCK"),
            )
            .child(card)
    }
}
