use std::ops::Range;

use gpui_kit::base::{ObservedElement, Scrollbar, ScrollbarMode, TestSupportExt};
use gpui_kit::{HighlightStyle, Role, ScrollWheelEvent, Stateful, StyledText, canvas};
use rusty_core::paging::HEX_WIDTH;

use super::PagedView;
use super::lines::{HexLine, PagedKind, Screen, TextLine, hex_lines, text_lines};
use crate::prelude::*;
use crate::tokens::Theme;

const SHOWN_CHARS: usize = 1000;
const MIN_DIGITS: usize = 4;

pub(super) enum Drawn {
    Hex(Screen<HexLine>),
    Text(Screen<TextLine>),
}

impl Drawn {
    fn end(&self) -> u64 {
        match self {
            Self::Hex(screen) => screen.end,
            Self::Text(screen) => screen.end,
        }
    }
}

struct RowLook<'a> {
    t: &'a Theme,
    height: Pixels,
    hit: Option<Range<u64>>,
}

fn row_frame(look: &RowLook<'_>, index: usize, label: String) -> ObservedElement<Stateful<Div>> {
    div()
        .id(("paged-row", index))
        .aria_label(label)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(look.t.spacing.xl)
        .h(look.height)
        .px(look.t.spacing.xl)
        .whitespace_nowrap()
        .overflow_hidden()
}

fn marked(look: &RowLook<'_>) -> HighlightStyle {
    HighlightStyle {
        background_color: Some(look.t.colors.selection),
        color: Some(look.t.colors.accent_text),
        ..HighlightStyle::default()
    }
}

fn hex_marks(look: &RowLook<'_>, start: u64) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let Some(hit) = &look.hit else {
        return (Vec::new(), Vec::new());
    };
    (0..HEX_WIDTH)
        .filter(|slot| hit.contains(&(start + slot)))
        .map(|slot| {
            let slot = usize::try_from(slot).unwrap_or(0);
            let at = slot * 3 + usize::from(slot >= 8);
            (at..at + 2, slot..slot + 1)
        })
        .unzip()
}

fn hex_row(look: &RowLook<'_>, index: usize, line: &HexLine) -> AnyElement {
    let (hex_marks, ascii_marks) = hex_marks(look, line.start);
    let style = marked(look);
    let row = &line.row;
    let label = format!("{}  {}  {}", row.offset, row.hex, row.ascii);
    row_frame(look, index, label)
        .child(
            div()
                .text_color(look.t.colors.muted)
                .child(row.offset.clone()),
        )
        .child(
            StyledText::new(row.hex.clone())
                .with_highlights(hex_marks.into_iter().map(|range| (range, style))),
        )
        .child(
            div().text_color(look.t.colors.muted).child(
                StyledText::new(row.ascii.clone())
                    .with_highlights(ascii_marks.into_iter().map(|range| (range, style))),
            ),
        )
        .into_any_element()
}

struct TextLook {
    digits: usize,
    shift: usize,
}

fn text_row(look: &RowLook<'_>, text: &TextLook, index: usize, line: &TextLine) -> AnyElement {
    let number = line
        .line
        .map(|number| number.to_string())
        .unwrap_or_default();
    let shown: String = line
        .text
        .chars()
        .skip(text.shift)
        .take(SHOWN_CHARS)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let is_hit = look
        .hit
        .as_ref()
        .is_some_and(|hit| (line.start..line.next.max(line.start + 1)).contains(&hit.start));
    row_frame(look, index, line.text.clone())
        .when(is_hit, |row| row.bg(look.t.colors.selection))
        .child(
            div()
                .id(("paged-line", index))
                .aria_label(number.clone())
                .test_support()
                .flex_none()
                .text_color(look.t.colors.muted)
                .child(format!("{number:>width$}", width = text.digits)),
        )
        .child(shown)
        .into_any_element()
}

impl PagedView {
    pub(super) fn draw(&self, count: usize) -> Result<Drawn, String> {
        let source = &*self.source;
        match self.kind {
            PagedKind::Hex => hex_lines(source, self.top, count).map(Drawn::Hex),
            PagedKind::Text => {
                text_lines(source, self.top, count, self.index.as_ref()).map(Drawn::Text)
            }
        }
    }

    fn digits(&self) -> usize {
        let lines = self.index.as_ref().map_or(0, |index| index.newlines() + 1);
        lines.to_string().len().max(MIN_DIGITS)
    }

    fn rows(&self, t: &Theme, drawn: &Drawn) -> Vec<AnyElement> {
        let look = RowLook {
            t,
            height: self.row_height,
            hit: self.find.hit.clone(),
        };
        match drawn {
            Drawn::Hex(screen) => screen
                .lines
                .iter()
                .enumerate()
                .map(|(index, line)| hex_row(&look, index, line))
                .collect(),
            Drawn::Text(screen) => {
                let text = TextLook {
                    digits: self.digits(),
                    shift: self.shift,
                };
                screen
                    .lines
                    .iter()
                    .enumerate()
                    .map(|(index, line)| text_row(&look, &text, index, line))
                    .collect()
            }
        }
    }

    pub(super) fn show_position(&self, drawn: Option<&Drawn>) {
        let len = self.source.len();
        let shown = drawn.map_or(0, |drawn| drawn.end().saturating_sub(self.top));
        if len == 0 || shown == 0 || (self.top == 0 && shown >= len) {
            self.scroll.show(0., 1.);
            return;
        }
        self.scroll
            .show(self.top as f64 / len as f64, len as f64 / shown as f64);
    }

    pub(super) fn body(
        &self,
        t: &Theme,
        drawn: Result<Drawn, String>,
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
        let content = match drawn {
            Ok(drawn) => div()
                .flex()
                .flex_col()
                .children(self.rows(t, &drawn))
                .into_any_element(),
            Err(error) => div()
                .id("paged-error")
                .role(Role::Alert)
                .aria_label(error.clone())
                .test_support()
                .p(t.spacing.xxl)
                .text_color(t.colors.mark)
                .child(error)
                .into_any_element(),
        };
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
            .child(content)
            .child(
                div().absolute().top_0().left_0().size_full().child(
                    Scrollbar::vertical(&self.scroll)
                        .id("paged-scrollbar")
                        .mode(ScrollbarMode::Always)
                        .viewport_from_layout(),
                ),
            )
    }
}
