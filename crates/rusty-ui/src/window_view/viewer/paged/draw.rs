use gpui_kit::base::{ObservedElement, Scrollbar, ScrollbarMode, TestSupportExt};
use gpui_kit::{Role, ScrollWheelEvent, Stateful, canvas};

use super::PagedView;
use super::moves::Move;
use super::row_view::{RowLook, TextLook, hex_row, missing_row, text_row};
use super::rows::{Screenful, Shown};
use super::spans::Painted;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::search::overview::{StripLook, TickScale};

const MIN_DIGITS: usize = 4;

struct ScreenMarks {
    marks: Vec<(std::ops::Range<u64>, u8)>,
    bookmarks: Vec<u64>,
}

impl PagedView {
    fn digits(&self) -> usize {
        let lines = self.index.as_ref().map_or(0, |index| index.newlines() + 1);
        lines.to_string().len().max(MIN_DIGITS)
    }

    fn screen_marks(&self, end: u64) -> ScreenMarks {
        let span = self.top..end.max(self.top + 1);
        let bookmarks = self
            .marking
            .bookmarks
            .iter()
            .filter(|at| span.contains(at))
            .collect();
        ScreenMarks {
            marks: self.marking.marks.ranges_in(span),
            bookmarks,
        }
    }

    fn rows(&self, t: &Theme, screen: &Screenful<'_>) -> Vec<AnyElement> {
        let marks = self.screen_marks(screen.end);
        let look = RowLook {
            t,
            height: self.row_height,
            painted: Painted {
                hit: self.find.hit.as_ref(),
                marks: &marks.marks,
            },
            bookmarks: &marks.bookmarks,
            line_ends: self.marking.line_ends,
        };
        let mut rows: Vec<AnyElement> = match &screen.shown {
            Shown::Hex(lines) => lines
                .iter()
                .enumerate()
                .map(|(index, line)| hex_row(&look, index, line))
                .collect(),
            Shown::Text(lines) => {
                let text = TextLook {
                    digits: self.digits(),
                    shift: self.shift,
                };
                lines
                    .iter()
                    .enumerate()
                    .map(|(index, line)| text_row(&look, &text, index, line))
                    .collect()
            }
            Shown::Failed(_) => Vec::new(),
        };
        let first = rows.len();
        rows.extend((first..first + screen.missing).map(|index| missing_row(&look, index)));
        rows
    }

    pub(super) fn show_position(&self, end: u64) {
        let len = self.source.len();
        let shown = end.saturating_sub(self.top);
        if len == 0 || shown == 0 || (self.top == 0 && shown >= len) {
            self.scroll.show(0., 1.);
            return;
        }
        self.scroll
            .show(self.top as f64 / len as f64, len as f64 / shown as f64);
    }

    pub(super) fn update_overview(&mut self, t: &Theme) {
        let scale = TickScale {
            len: self.source.len(),
            rows: self.overview.rows(t),
            place: &|offset| offset,
        };
        self.marking.update_ticks(&scale);
    }

    fn overview_click(&mut self, y: Pixels, t: &Theme, cx: &mut Context<Self>) {
        let rows = self.overview.rows(t);
        if rows == 0 {
            return;
        }
        let row = self.overview.row_at(t, y);
        let to = match self.marking.tick_target(row) {
            Some(offset) => Move::Offset(offset),
            None => Move::Fraction(f64::from(row) / f64::from(rows)),
        };
        self.user_move(to, cx);
    }

    fn overview(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let theme = t.clone();
        let look = StripLook {
            t,
            shown: self.marking.ticks(),
            entity: cx.entity_id(),
        };
        self.overview.element(look, move |y, _, cx| {
            let _ = this.update(cx, |view, cx| view.overview_click(y, &theme, cx));
        })
    }

    fn content(&self, t: &Theme, screen: &Screenful<'_>) -> AnyElement {
        match screen.shown {
            Shown::Failed(error) => div()
                .id("paged-error")
                .role(Role::Alert)
                .aria_label(error.to_owned())
                .test_support()
                .p(t.spacing.xxl)
                .text_color(t.colors.mark)
                .child(SharedString::from(error.to_owned()))
                .into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .children(self.rows(t, screen))
                .into_any_element(),
        }
    }

    pub(super) fn body(
        &self,
        t: &Theme,
        screen: &Screenful<'_>,
        cx: &mut Context<Self>,
    ) -> ObservedElement<Stateful<Div>> {
        let scroll = self.scroll.clone();
        let entity = cx.entity_id();
        let measure = canvas(
            move |bounds, window, _| {
                if scroll.measure(bounds) {
                    window.on_next_frame(move |_, cx| cx.notify(entity));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        div()
            .id("paged-body")
            .test_support()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.small)
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
                view.scroll_wheel(event, cx);
            }))
            .child(measure)
            .child(self.content(t, screen))
            .child(
                div().absolute().top_0().left_0().size_full().child(
                    Scrollbar::vertical(&self.scroll)
                        .id("paged-scrollbar")
                        .mode(ScrollbarMode::Always)
                        .viewport_from_layout(),
                ),
            )
            .child(self.overview(t, cx))
    }
}
