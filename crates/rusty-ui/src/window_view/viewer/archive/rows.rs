use gpui_kit::{FontWeight, Hsla, Pixels};
use rusty_fs::EntryKind;
use rusty_fs::archive::{ArchiveEntry, Stamp};

use crate::file_list::kind;
use crate::file_list::row::inset_ring;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

pub(super) struct Look<'a> {
    pub theme: &'a Theme,
    pub entry: &'a ArchiveEntry,
    pub row: usize,
    pub is_cursor: bool,
    pub has_packed: bool,
}

pub(super) fn date(stamp: Option<Stamp>) -> String {
    match stamp {
        Some(Stamp::UnixMs(ms)) => format::date(ms),
        Some(Stamp::Local(at)) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            at.year, at.month, at.day, at.hour, at.minute
        ),
        None => String::new(),
    }
}

pub(super) fn display_name(entry: &ArchiveEntry) -> &str {
    if entry.is_folder {
        let trimmed = entry.name.trim_end_matches('/');
        if trimmed.is_empty() {
            &entry.name
        } else {
            trimmed
        }
    } else {
        &entry.name
    }
}

fn size_text(entry: &ArchiveEntry) -> String {
    if entry.is_folder {
        "<DIR>".to_owned()
    } else {
        format::size(entry.size)
    }
}

pub(super) fn spoken(entry: &ArchiveEntry) -> String {
    let name = display_name(entry);
    let mut words = if entry.is_folder {
        format!("{name}, folder")
    } else {
        format!("{name}, {} bytes", format::bytes(entry.size))
    };
    let date = date(entry.modified);
    if !entry.is_folder && !date.is_empty() {
        words.push_str(", ");
        words.push_str(&date);
    }
    if entry.is_encrypted {
        words.push_str(", encrypted");
    }
    words
}

fn glyph(t: &Theme, entry: &ArchiveEntry) -> (IconName, Hsla) {
    if entry.is_encrypted {
        return (IconName::Lock, t.colors.warn);
    }
    let kind = if entry.is_folder {
        EntryKind::Folder
    } else {
        EntryKind::File
    };
    let name = display_name(entry);
    let leaf = name.rsplit('/').next().unwrap_or(name);
    let (_, extension) = kind::split_name(leaf, kind);
    kind::icon(kind, extension, &t.colors)
}

pub(super) fn right(width: Pixels, text: impl Into<SharedString>) -> Div {
    div()
        .w(width)
        .flex_none()
        .flex()
        .justify_end()
        .overflow_hidden()
        .child(text.into())
}

pub(super) fn columns(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .w_full()
        .pl(t.spacing.xl)
        .pr(t.spacing.xxl)
}

fn name_cell(look: &Look) -> Div {
    let t = look.theme;
    let entry = look.entry;
    let (glyph, color) = glyph(t, entry);
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .flex_1()
        .min_w_0()
        .child(icon(glyph, t.spacing.icon - px(1.), color))
        .child(
            div()
                .min_w_0()
                .truncate()
                .when(entry.is_folder, |d| d.font_weight(FontWeight::SEMIBOLD))
                .when(entry.is_encrypted, |d| d.text_color(t.colors.muted))
                .child(display_name(entry).to_owned()),
        )
}

pub(super) fn cells(look: &Look) -> Div {
    let theme = look.theme.selected_content(look.is_cursor);
    let look = &Look {
        theme: &theme,
        ..*look
    };
    let t = look.theme;
    let c = &t.colors;
    let entry = look.entry;
    let packed = entry
        .packed
        .filter(|_| !entry.is_folder)
        .map(format::size)
        .unwrap_or_default();
    let numbers = div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .flex_none()
        .font_family(t.typography.numbers_font.clone())
        .font_features(t.typography.numbers_font_features.clone())
        .text_size(t.typography.small)
        .text_color(c.muted)
        .child(right(t.spacing.size_column, size_text(entry)))
        .when(look.has_packed, |d| {
            d.child(right(t.spacing.size_column, packed))
        })
        .child(right(
            t.spacing.date_column + t.spacing.xl,
            date(entry.modified),
        ));
    columns(t)
        .h_full()
        .text_color(c.text)
        .when(look.row % 2 == 1, |d| d.bg(c.zebra))
        .when(look.is_cursor, |d| {
            d.bg(c.selection).shadow(inset_ring(c.accent))
        })
        .child(name_cell(look))
        .child(numbers)
}
