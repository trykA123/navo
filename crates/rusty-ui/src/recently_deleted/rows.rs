use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{AnyElement, Hsla, Stateful, StatefulInteractiveElement};
use rusty_core::retention::RetentionId;
use rusty_fs::EntryKind;
use rusty_l10n::tr;

use super::entries::{DayHead, Entry, Model};
use super::text;
use super::{Event, Send};
use crate::browser::{RetainedRow, RetainedState};
use crate::file_list::kind;
use crate::file_list::row::inset_ring;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconButton, IconName, icon};
use crate::tokens::Theme;

pub(super) struct Frame {
    pub cursor: usize,
    pub now_ms: i64,
    pub working: Rc<Vec<RetentionId>>,
    pub send: Option<Send>,
}

pub(super) fn height(t: &Theme) -> Pixels {
    t.typography.line_height * 2. + t.spacing.md * 2.
}

fn line(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .min_w_0()
        .h(t.typography.line_height)
}

fn glyph_column(t: &Theme, name: IconName, color: Hsla) -> Div {
    line(t)
        .flex_none()
        .justify_center()
        .w(t.spacing.icon)
        .child(icon(name, t.spacing.icon, color))
}

pub(super) fn day_head(t: &Theme, head: &DayHead) -> Div {
    let label = div()
        .truncate()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text::day_label(head.day));
    let summary = div()
        .truncate()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text::summary(head.count, head.bytes));
    div()
        .flex()
        .items_start()
        .gap(t.spacing.md)
        .w_full()
        .h_full()
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .border_b_1()
        .border_color(t.colors.line)
        .child(glyph_column(t, IconName::Calendar, t.colors.muted))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(line(t).child(label))
                .child(line(t).child(summary)),
        )
}

fn pill(t: &Theme, label: String, color: Hsla) -> Div {
    div()
        .flex_none()
        .px(t.spacing.sm)
        .rounded(t.radii.pill)
        .border_1()
        .border_color(t.colors.line)
        .text_size(t.typography.tiny)
        .line_height(t.typography.tiny)
        .text_color(color)
        .child(label)
}

fn pills(t: &Theme, row: &RetainedRow, is_working: bool) -> Vec<Div> {
    let state = if is_working {
        Some((tr("recently-deleted-working"), t.colors.muted))
    } else {
        text::state_label(row.state).map(|label| {
            let color = match row.state {
                RetainedState::NeedsAttention => t.colors.warn,
                _ => t.colors.accent_text,
            };
            (label, color)
        })
    };
    let kind = text::kind_label(row.kind).map(|label| (label, t.colors.muted));
    kind.into_iter()
        .chain(state)
        .map(|(label, color)| pill(t, label, color))
        .collect()
}

fn meta(t: &Theme, row: &RetainedRow, now_ms: i64) -> Div {
    let left = row.expires_at_ms.saturating_sub(now_ms);
    let expires = if left < text::SOON_MS {
        t.colors.warn
    } else {
        t.colors.muted
    };
    div()
        .flex()
        .flex_col()
        .items_end()
        .flex_none()
        .font_family(t.typography.numbers_font.clone())
        .font_features(t.typography.numbers_font_features.clone())
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(line(t).child(format::size(row.bytes)))
        .child(line(t).text_color(expires).child(text::expires(left)))
}

fn actions(t: &Theme, index: usize, send: Option<&Send>) -> Div {
    let button = |id: &'static str, glyph: IconName, label: String, event: Event| {
        let send = send.cloned();
        IconButton::new((id, index), glyph, label).on_click(move |_, window, cx| {
            if let Some(send) = &send {
                send(event, window, cx);
            }
        })
    };
    div()
        .flex()
        .items_center()
        .flex_none()
        .self_center()
        .gap(t.spacing.xs)
        .child(button(
            "recent-restore",
            IconName::Undo2,
            tr("recently-deleted-restore"),
            Event::Restore,
        ))
        .child(button(
            "recent-delete",
            IconName::Trash,
            tr("recently-deleted-delete"),
            Event::Delete,
        ))
}

fn name_column(t: &Theme, row: &RetainedRow, is_working: bool) -> Div {
    let title = line(t)
        .gap(t.spacing.md)
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .child(text::name_of(&row.original)),
        )
        .children(pills(t, row, is_working));
    let folder = line(t)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.tiny)
        .text_color(t.colors.muted)
        .child(div().truncate().child(text::folder_of(&row.original)));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(title)
        .child(folder)
}

fn item(t: &Theme, (index, row): (usize, &RetainedRow), frame: &Frame) -> Stateful<Div> {
    let is_cursor = frame.cursor == index;
    let is_working = frame.working.contains(&row.id);
    let themed = t.selected_content(is_cursor);
    let t = &*themed;
    let name = text::name_of(&row.original);
    let (_, extension) = kind::split_name(&name, EntryKind::File);
    let (glyph, tone) = kind::icon(EntryKind::File, extension, &t.colors);
    let side = if is_cursor && !is_working {
        actions(t, index, frame.send.as_ref())
    } else {
        meta(t, row, frame.now_ms)
    };
    let select = frame.send.clone();
    div()
        .id(("recent-item", index))
        .flex()
        .items_start()
        .gap(t.spacing.md)
        .w_full()
        .h_full()
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .border_b_1()
        .border_color(t.colors.line)
        .text_color(t.colors.text)
        .cursor_pointer()
        .when(is_cursor, |row| {
            row.bg(t.colors.selection)
                .shadow(inset_ring(t.colors.accent))
        })
        .when(!is_cursor, |row| row.hover(|row| row.bg(t.colors.hover)))
        .on_click(move |_, window, cx| {
            if let Some(select) = &select {
                select(Event::Select(index), window, cx);
            }
        })
        .child(glyph_column(t, glyph, tone))
        .child(name_column(t, row, is_working))
        .child(side)
}

fn element(t: &Theme, index: usize, entry: &Entry, frame: &Frame) -> AnyElement {
    match entry {
        Entry::Day(head) => day_head(t, head).into_any_element(),
        Entry::Item(row) => item(t, (index, row), frame).into_any_element(),
    }
}

pub(super) fn slice(
    t: &Theme,
    range: Range<usize>,
    (model, frame): (&Model, &Frame),
) -> Vec<AnyElement> {
    let entries = model.entries();
    range
        .filter_map(|at| Some(element(t, at, entries.get(at)?, frame)))
        .collect()
}
