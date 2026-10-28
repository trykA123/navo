use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    BoxShadow, Div, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, Role,
    SharedString, Stateful, StatefulInteractiveElement, Styled, div, point, px,
};
use rusty_fs::{EntryKind, FsEntry};

use super::widths::ColumnWidths;
use super::{ListParts, SortBy, kind};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

const HIDDEN_ICON_OPACITY: f32 = 0.55;

pub(super) struct RowLook<'a> {
    pub theme: &'a Theme,
    pub entry: &'a FsEntry,
    pub is_cursor: bool,
    pub is_marked: bool,
    pub is_active_pane: bool,
    pub parts: ListParts,
    pub widths: ColumnWidths,
    pub is_find: bool,
}

fn fixed_width(look: &RowLook, by: SortBy) -> Pixels {
    look.widths.of(by).unwrap_or_default()
}

pub(super) fn check_column(t: &Theme) -> Pixels {
    t.spacing.xxl + t.spacing.xs
}

pub(crate) fn inset_ring(color: Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(1.),
        inset: true,
    }]
}

pub(super) fn columns(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .mx(t.spacing.xs)
        .pl(t.rows.inset)
        .pr(t.spacing.xxl)
}

fn check_cell(look: &RowLook) -> Div {
    let t = look.theme;
    div()
        .w(check_column(t))
        .flex_none()
        .flex()
        .justify_center()
        .when(look.is_marked, |d| {
            d.child(icon(
                IconName::CircleCheck,
                t.spacing.icon - px(1.),
                t.colors.mark,
            ))
        })
}

pub(crate) fn cursor_bar(t: &Theme) -> impl IntoElement + use<> {
    div()
        .id("cursor-bar")
        .test_support()
        .absolute()
        .left_0()
        .top(t.spacing.xs)
        .bottom(t.spacing.xs)
        .w(px(3.))
        .rounded(t.radii.xs)
        .bg(t.surfaces.card_ring)
}

fn right_cell(column: usize, width: Pixels, text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .id(("cell", column))
        .test_support()
        .w(width)
        .flex_none()
        .flex()
        .justify_end()
        .overflow_hidden()
        .child(
            div()
                .id(("cell-text", column))
                .test_support()
                .min_w_0()
                .truncate()
                .child(text.into()),
        )
}

pub(super) fn name_text(
    (name, extension): (&str, &str),
    is_folder: bool,
    shows_ext: bool,
) -> String {
    match (is_folder, shows_ext || extension.is_empty()) {
        (true, _) => format!("[{name}]"),
        (false, true) => name.to_owned(),
        (false, false) => format!("{name}.{extension}"),
    }
}

fn name_cell(
    look: &RowLook,
    (name, extension): (&str, &str),
    (grip, zone): (Stateful<Div>, Div),
) -> impl IntoElement {
    let t = look.theme;
    let c = &t.colors;
    let entry = look.entry;
    let (glyph, glyph_color) = kind::icon(entry.kind, extension, c);
    let is_folder = entry.kind == EntryKind::Folder;
    let text = name_text((name, extension), is_folder, look.parts.ext);
    let glyph = div()
        .id("row-icon")
        .test_support()
        .flex_none()
        .when(entry.attributes.hidden && !t.is_contrast, |d| {
            d.opacity(HIDDEN_ICON_OPACITY)
        })
        .child(icon(glyph, t.spacing.icon - px(1.), glyph_color));
    div()
        .id(("cell", 0usize))
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .flex_1()
        .min_w_0()
        .h_full()
        .child(
            grip.test_support()
                .flex()
                .items_center()
                .gap(t.spacing.md)
                .min_w_0()
                .h_full()
                .when(look.parts.icons, |d| d.child(glyph))
                .child(
                    div()
                        .id("find-name")
                        .aria_label(text.clone())
                        .test_support()
                        .min_w_0()
                        .truncate()
                        .when(is_folder || look.is_marked, |d| {
                            d.font_weight(FontWeight::SEMIBOLD)
                        })
                        .child(text),
                ),
        )
        .child(zone.flex_1().h_full())
}

fn name_color(look: &RowLook) -> Hsla {
    let c = &look.theme.colors;
    match (look.is_marked, look.entry.attributes.hidden) {
        (true, _) => c.mark,
        (false, true) => c.muted,
        (false, false) => c.text,
    }
}

fn numbers(look: &RowLook) -> Div {
    let t = look.theme;
    let c = &t.colors;
    let entry = look.entry;
    let number_color = if look.is_marked { c.mark } else { c.muted };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .flex_none()
        .font_family(t.typography.numbers_font.clone())
        .font_features(t.typography.numbers_font_features.clone())
        .text_size(t.typography.small)
        .text_color(number_color)
        .when(look.is_marked, |d| d.font_weight(FontWeight::SEMIBOLD))
        .when(look.parts.size, |d| {
            d.child(right_cell(
                2,
                fixed_width(look, SortBy::Size),
                kind::size_text(entry),
            ))
        })
        .when(look.parts.date, |d| {
            d.child(right_cell(
                3,
                fixed_width(look, SortBy::Date),
                kind::date_text(entry),
            ))
        })
}

pub(super) fn cells(look: &RowLook, zones: (Stateful<Div>, Div)) -> Div {
    let theme = look
        .theme
        .selected_content(look.is_cursor && look.is_active_pane);
    let look = &RowLook {
        theme: &theme,
        ..*look
    };
    let t = look.theme;
    let c = &t.colors;
    let entry = look.entry;
    let display = entry.name.display();
    let (display, folder) = if look.is_find {
        super::find::split(&display)
    } else {
        (display.as_ref(), "")
    };
    let (name, extension) = kind::split_name(display, entry.kind);
    let is_active_cursor = look.is_cursor && look.is_active_pane;
    let is_idle_cursor = look.is_cursor && !look.is_active_pane;
    columns(t)
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .rounded(t.radii.md)
        .text_color(name_color(look))
        .when(is_active_cursor, |d| {
            d.bg(t.surfaces.current).child(cursor_bar(t))
        })
        .when(is_idle_cursor, |d| d.shadow(inset_ring(c.line)))
        .child(check_cell(look))
        .child(name_cell(look, (name, extension), zones))
        .when(look.is_find, |d| {
            d.child(super::find::folder_cell("find-folder", folder, t))
        })
        .when(look.parts.ext, |d| {
            d.child(
                div()
                    .id(("cell", 1usize))
                    .test_support()
                    .w(fixed_width(look, SortBy::Ext))
                    .flex_none()
                    .truncate()
                    .when(look.is_marked, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .child(extension.to_owned()),
            )
        })
        .when(look.parts.size || look.parts.date, |d| {
            d.child(numbers(look))
        })
}

pub(crate) fn filter_bar(t: &Theme, text: SharedString, shown: usize) -> impl IntoElement {
    let c = &t.colors;
    let count = crate::format::count(u64::try_from(shown).unwrap_or(u64::MAX), "item", "items");
    let spoken: SharedString = format!("Filter {text}, {count}").into();
    div()
        .id("filter")
        .role(Role::Status)
        .aria_label(spoken)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .h(t.spacing.row_height)
        .px(t.spacing.xl)
        .bg(c.accent.opacity(0.12))
        .border_t_1()
        .border_color(c.accent)
        .text_size(t.typography.small)
        .child(icon(IconName::Search, px(12.), c.accent_text))
        .child(
            div()
                .font_family(t.typography.mono_font.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c.text)
                .child(text),
        )
        .child(div().text_color(c.muted).child(count))
        .child(div().flex_1())
        .child(div().text_color(c.faint).child("Esc clears"))
}
