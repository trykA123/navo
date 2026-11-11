//! Gallery pages `p42-diff*`: the compare file diff and the Stream with fake data, wide
//! (side by side, 1280 px) and narrow (unified, 720 px), with their drawers open.

use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use navo_ui::compare_diff::body::{BodyLook, digits, row_view};
use navo_ui::compare_diff::map::{ChangeMap, HunkKind, MapHunk};
use navo_ui::compare_diff::{
    CardCtx, DiffRow, Layout, PairText, PickSide, SpanCache, StreamModel, StreamRow, Ticks,
    line_on, pair_rows, stream_row_view, stream_rows,
};
use navo_ui::prelude::*;
use navo_ui::tokens::{self, Theme};

use crate::compare_diff_parts as parts;
use crate::compare_diff_sample as sample;

/// Width of the narrow pages, in px.
const NARROW: f32 = 720.;
/// Extra frames the page asks for so the change map shows its ticks in a screenshot.
const FRAMES: u8 = 4;
/// The change shown as current in the file diff (0-based).
const CURRENT: usize = 1;

/// Which screen a page shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    File,
    Stream,
}

/// One gallery page: a screen at a width.
pub struct CompareDiffPage {
    screen: Screen,
    narrow: bool,
    text: Arc<PairText>,
    spans: SpanCache,
    stream: StreamModel,
    map: Entity<ChangeMap>,
    /// Frames asked for so far: the change map measures itself on the first paints.
    frames: u8,
}

/// The map's hunks: row ranges as `layout` shows them, unified counting a changed row twice.
fn map_hunks(text: &PairText, layout: Layout) -> Arc<[MapHunk]> {
    let mut extra = 0;
    text.hunks
        .iter()
        .map(|hunk| {
            let rows = &text.rows[hunk.rows.start as usize..hunk.rows.end as usize];
            let doubled = match layout {
                Layout::Unified => rows
                    .iter()
                    .filter(|row| matches!(row, DiffRow::Changed(..)))
                    .count() as u32,
                Layout::SideBySide => 0,
            };
            let kind = if rows.iter().all(|r| matches!(r, DiffRow::Inserted(_))) {
                HunkKind::Added
            } else if rows.iter().all(|r| matches!(r, DiffRow::Deleted(_))) {
                HunkKind::Deleted
            } else {
                HunkKind::Changed
            };
            let start = hunk.rows.start + extra;
            extra += doubled;
            MapHunk {
                rows: start..hunk.rows.end + extra,
                kind,
            }
        })
        .collect()
}

/// The Stream's cards as one padded column on the rail colour.
fn card_list(t: &Theme, model: &StreamModel, layout: Layout) -> Div {
    let rows = stream_rows(model, layout);
    let ctx = CardCtx {
        t,
        model,
        layout,
        bars: None,
    };
    let cards = (0..rows.len()).map(|at| {
        let first = matches!(rows[at], StreamRow::Header(_) | StreamRow::RulesGroup);
        div()
            .px(t.spacing.lg)
            .when(first, |d| d.pt(t.spacing.md))
            .child(stream_row_view(&rows, at, &ctx))
    });
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .bg(t.colors.rail)
        .children(cards)
}

impl CompareDiffPage {
    /// A page showing `screen`, narrow or wide.
    pub fn new(screen: Screen, narrow: bool, cx: &mut Context<Self>) -> Self {
        let (text, spans) = sample::pair();
        let layout = if narrow {
            Layout::Unified
        } else {
            Layout::SideBySide
        };
        let hunks = map_hunks(&text, layout);
        let total = pair_rows(&text, layout, 0..text.rows.len() * 2).len() as u32;
        let map = cx.new(|cx| {
            let mut map = ChangeMap::new(hunks, total);
            map.set_viewport(0..total, cx);
            map.set_current(Some(CURRENT), cx);
            map
        });
        Self {
            screen,
            narrow,
            text,
            spans,
            stream: sample::stream(),
            map,
            frames: 0,
        }
    }

    /// The left lines of the current change, as if picked with Shift+↓ (V32).
    fn picked(&self) -> BTreeSet<u32> {
        let rows = &self.text.hunks[CURRENT].rows;
        let rows = &self.text.rows[rows.start as usize..rows.end as usize];
        rows.iter()
            .filter_map(|row| line_on(*row, PickSide::Left))
            .collect()
    }

    /// The file diff: header, drawers, rows, change map and status line.
    fn file(&self, t: &Theme, layout: Layout) -> Div {
        let picked = self.picked();
        let ticks = Ticks {
            text: &self.text,
            side: Some(PickSide::Left),
            cursor: picked.iter().next_back().copied(),
            picked: &picked,
            hovered: None,
            on_tick: Rc::new(|_, _, _, _, _| {}),
        };
        let look = BodyLook {
            theme: t,
            layout,
            digits: digits(&self.text),
            current: Some(CURRENT as u32),
            spans: Some(&self.spans),
            ticks: Some(&ticks),
            bytes_here: None,
        };
        let rows = pair_rows(&self.text, layout, 0..self.text.rows.len() * 2);
        let list = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .children(
                rows.iter()
                    .map(|row| div().h(t.spacing.row_height).child(row_view(&look, row))),
            );
        let body = div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(parts::changes(t, &self.text, CURRENT))
            .child(list)
            .when(!self.narrow, |body| body.child(parts::details(t)))
            .child(self.map.clone());
        let meta = "crates/core/src/ · left ↔ right";
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(parts::header(t, "walk.rs", meta, "UTF-8"))
            .child(body)
            .child(parts::status_line(t, "Change 2 of 3", None))
    }

    /// The Stream: header, outline drawer, six cards and the Viewed line.
    fn stream(&self, t: &Theme, layout: Layout) -> Div {
        let list = card_list(t, &self.stream, layout);
        let (viewed, total) = self.stream.counter();
        let counter = format!("Viewed {viewed} of {total}");
        let body = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .when(!self.narrow, |body| {
                body.child(parts::outline(t, &self.stream, 3, false))
            })
            .child(list)
            .when(self.narrow, |body| {
                body.child(parts::outline(t, &self.stream, 3, true))
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(parts::header(t, "Stream", "left ↔ right", "6 files"))
            .child(body)
            .child(parts::status_line(
                t,
                &counter,
                Some(viewed as f32 / total as f32),
            ))
    }
}

impl Render for CompareDiffPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.frames < FRAMES {
            self.frames += 1;
            window.request_animation_frame();
        }
        let t = tokens::current(cx).clone();
        let layout = if self.narrow {
            Layout::Unified
        } else {
            Layout::SideBySide
        };
        let screen = match self.screen {
            Screen::File => self.file(&t, layout),
            Screen::Stream => self.stream(&t, layout),
        };
        let panel = div()
            .flex()
            .flex_col()
            .min_h_0()
            .h_full()
            .rounded(t.radii.card)
            .overflow_hidden()
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.colors.surface)
            .when(self.narrow, |d| d.w(px(NARROW)))
            .when(!self.narrow, |d| d.w_full())
            .child(screen);
        div().flex().flex_1().min_h_0().p(t.spacing.md).child(panel)
    }
}
