use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::{InteractiveText, MouseButton, StyledText, WeakEntity};
use rusty_structured::markdown::{Block, BlockKind, Document, Marker};

use super::MarkdownView;
use super::runs::{RunLook, Runs, runs};
use super::table::{TableScrolls, table_row};
use crate::prelude::*;
use crate::primitives::icon;
use crate::tokens::Theme;

const HEADING_SCALE: [f32; 6] = [1.75, 1.45, 1.2, 1.05, 1., 1.];
const RULE_HEIGHT: f32 = 1.;
const TEXT_COLUMN: f32 = 3.5;

pub(super) fn text_column(t: &Theme) -> Pixels {
    t.spacing.rail_width * TEXT_COLUMN
}

#[derive(Clone)]
pub(super) struct Page {
    pub t: Theme,
    pub doc: Rc<Document>,
    pub picked: Option<u32>,
    pub view: WeakEntity<MarkdownView>,
    pub tables: Rc<TableScrolls>,
}

pub(super) fn text_element(page: &Page, id: (&'static str, usize), shown: Runs) -> AnyElement {
    let styled = StyledText::new(shown.text).with_runs(shown.runs);
    if shown.links.is_empty() {
        return styled.into_any_element();
    }
    let (ranges, ids): (Vec<_>, Vec<u32>) = shown.links.into_iter().unzip();
    let view = page.view.clone();
    InteractiveText::new(id, styled)
        .on_click(ranges, move |at, window, cx| {
            let Some(link) = ids.get(at).copied() else {
                return;
            };
            let opens = window.modifiers().secondary();
            let _ = view.update(cx, |view, cx| view.click_link(link, opens, cx));
        })
        .into_any_element()
}

fn look<'a>(page: &'a Page, block: &Block) -> RunLook<'a> {
    let is_heading = matches!(block.kind, BlockKind::Heading(_));
    RunLook {
        t: &page.t,
        is_mono: matches!(block.kind, BlockKind::Code { .. } | BlockKind::Html),
        weight: if is_heading {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        },
        color: if block.quote > 0 || block.kind == BlockKind::Html {
            page.t.colors.muted
        } else {
            page.t.colors.text
        },
        picked: page.picked,
    }
}

fn is_last_chunk(doc: &Document, index: usize) -> bool {
    doc.blocks()
        .get(index + 1)
        .is_none_or(|next| !next.is_continued)
}

fn code(page: &Page, block: &Block, index: usize) -> AnyElement {
    let t = &page.t;
    let shown = runs(&page.doc, block, None, &look(page, block));
    let language = match block.kind {
        BlockKind::Code {
            language: Some(language),
        } if !block.is_continued => page.doc.label(language),
        _ => None,
    };
    let is_last = is_last_chunk(&page.doc, index);
    div()
        .w_full()
        .flex()
        .flex_col()
        .px(t.spacing.xl)
        .bg(t.colors.rail)
        .text_size(t.typography.small)
        .when(!block.is_continued, |code| {
            code.rounded_t(t.radii.md).pt(t.spacing.md)
        })
        .when(is_last, |code| code.rounded_b(t.radii.md).pb(t.spacing.md))
        .when_some(language, |code, language| {
            code.child(
                div()
                    .text_size(t.typography.tiny)
                    .text_color(t.colors.muted)
                    .child(SharedString::from(language.to_owned())),
            )
        })
        .child(
            div()
                .id(("md-code", index))
                .test_support()
                .flex()
                .w_full()
                .overflow_x_scroll()
                .child(
                    div()
                        .id(("md-code-text", index))
                        .test_support()
                        .flex_none()
                        .whitespace_nowrap()
                        .child(text_element(page, ("md-text", index), shown)),
                ),
        )
        .into_any_element()
}

fn heading(page: &Page, block: &Block, level: u8, index: usize) -> AnyElement {
    let t = &page.t;
    let scale = HEADING_SCALE
        .get(usize::from(level.saturating_sub(1)))
        .copied()
        .unwrap_or(1.);
    let shown = runs(&page.doc, block, None, &look(page, block));
    div()
        .w_full()
        .text_size(t.typography.body * scale)
        .line_height(t.typography.line_height * scale)
        .when(level <= 2 && is_last_chunk(&page.doc, index), |title| {
            title
                .pb(t.spacing.sm)
                .border_b_1()
                .border_color(t.colors.line)
        })
        .child(text_element(page, ("md-text", index), shown))
        .into_any_element()
}

fn content(page: &Page, block: &Block, (index, window): (usize, &Window)) -> AnyElement {
    match block.kind {
        BlockKind::Heading(level) => heading(page, block, level, index),
        BlockKind::Code { .. } | BlockKind::Html => code(page, block, index),
        BlockKind::Row { table, is_head } => {
            table_row(page, block, (table, is_head), (index, window))
        }
        BlockKind::Rule => div()
            .w_full()
            .my(page.t.spacing.md)
            .h(px(RULE_HEIGHT))
            .bg(page.t.colors.line)
            .into_any_element(),
        BlockKind::Paragraph => {
            let shown = runs(&page.doc, block, None, &look(page, block));
            div()
                .w_full()
                .child(text_element(page, ("md-text", index), shown))
                .into_any_element()
        }
    }
}

fn task_box(t: &Theme, is_done: bool) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(t.spacing.xl)
        .mt(t.spacing.xxs)
        .rounded(t.radii.xs)
        .border_1()
        .border_color(if is_done {
            t.colors.accent
        } else {
            t.colors.line
        })
        .when(is_done, |done| {
            done.bg(t.colors.accent)
                .child(icon(IconName::Check, t.spacing.lg, t.colors.surface))
        })
}

fn marker(t: &Theme, page: &Page, block: &Block) -> Option<Div> {
    let column = div()
        .flex_none()
        .min_w(t.spacing.xxl + t.spacing.md)
        .pr(t.spacing.sm)
        .text_color(t.colors.muted);
    let marker = match block.marker? {
        Marker::Bullet if block.list.is_multiple_of(2) => column.child("◦"),
        Marker::Bullet => column.child("•"),
        Marker::Number(number) => column
            .font_family(t.typography.numbers_font.clone())
            .child(format!("{number}.")),
        Marker::Task(is_done) => column.child(task_box(t, is_done)),
        Marker::Footnote(label) => column
            .text_color(t.colors.accent_text)
            .child(format!("[{}]", page.doc.label(label).unwrap_or_default())),
    };
    Some(marker)
}

fn quoted(t: &Theme, depth: u8, inner: Div) -> Div {
    (0..depth).fold(inner, |inner, _| {
        div()
            .flex()
            .w_full()
            .border_l_2()
            .border_color(t.colors.line)
            .pl(t.spacing.xl)
            .child(inner)
    })
}

fn gap(t: &Theme, doc: &Document, index: usize, block: &Block) -> Pixels {
    let previous = index.checked_sub(1).and_then(|at| doc.blocks().get(at));
    match block.kind {
        _ if block.is_continued => px(0.),
        BlockKind::Row { is_head: false, .. } => px(0.),
        BlockKind::Heading(level) if level <= 2 => t.spacing.xxl + t.spacing.md,
        BlockKind::Heading(_) => t.spacing.xxl,
        _ if block.list > 0 && previous.is_some_and(|previous| previous.list > 0) => t.spacing.xs,
        _ => t.spacing.xl,
    }
}

pub(super) fn draw_block(page: &Page, index: usize, window: &Window) -> AnyElement {
    let Some(block) = page.doc.blocks().get(index).copied() else {
        return div().into_any_element();
    };
    let t = &page.t;
    let marker = marker(t, page, &block);
    let indent = (t.spacing.xxl + t.spacing.md) * f32::from(block.list);
    let indent = if marker.is_some() {
        indent - (t.spacing.xxl + t.spacing.md)
    } else {
        indent
    };
    let body = div()
        .flex()
        .w_full()
        .pl(indent.max(px(0.)))
        .children(marker)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(content(page, &block, (index, window))),
        );
    let is_end = index + 1 == page.doc.blocks().len();
    let view = page.view.clone();
    div()
        .id(("md-block", index))
        .test_support()
        .on_mouse_down(MouseButton::Right, move |_, _, cx| {
            let _ = view.update(cx, |view, _| view.point_block(index));
        })
        .w_full()
        .flex()
        .justify_center()
        .pt(gap(t, &page.doc, index, &block))
        .when(is_end, |end| end.pb(t.spacing.xxl * 2.))
        .child(
            div()
                .w_full()
                .when(!matches!(block.kind, BlockKind::Row { .. }), |column| {
                    column.max_w(text_column(t))
                })
                .px(t.spacing.xxl)
                .child(quoted(t, block.quote, body)),
        )
        .into_any_element()
}
