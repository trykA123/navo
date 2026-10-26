use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, Hsla, ImageSource, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, Stateful, StatefulInteractiveElement, Styled, div, img, px,
};
use rusty_core::shell_menu::{ItemId, ItemRow, MenuItem, Row, ShellMenu};

use super::icons::{ShellIcons, ferrite_icon};
use super::place::{ROW, SEPARATOR};
use rusty_l10n::tr;

use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

const ICON_COLUMN: f32 = 20.;
const SHELL_ICON: f32 = 16.;

pub(super) type RowElement = ObservedElement<Stateful<Div>>;

pub(super) struct RowLook<'a> {
    pub theme: &'a Theme,
    pub menu: &'a ShellMenu,
    pub icons: &'a ShellIcons,
}

fn base(index: usize) -> RowElement {
    div()
        .id(("menu-row", index))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
}

fn line(t: &Theme, index: usize, is_selected: bool) -> RowElement {
    base(index)
        .role(Role::MenuItem)
        .aria_selected(is_selected)
        .gap(t.spacing.md)
        .h(px(ROW))
        .px(t.spacing.sm)
        .rounded(t.radii.md)
        .when(is_selected, |row| row.bg(t.colors.selection))
}

pub(super) fn icon_cell(child: Option<gpui_kit::AnyElement>) -> Div {
    div()
        .w(px(ICON_COLUMN))
        .flex_none()
        .flex()
        .justify_center()
        .children(child)
}

pub(super) fn render(look: &RowLook, row: &Row, index: usize, is_selected: bool) -> RowElement {
    let t = look.theme;
    match row {
        Row::Separator => base(index)
            .h(px(SEPARATOR))
            .px(t.spacing.sm)
            .child(div().h(px(1.)).w_full().bg(t.colors.line)),
        Row::Item(item) => item_row(look, item, index, is_selected),
        Row::Back { title } => base(index).aria_label(title.clone()),
    }
}

fn item_row(look: &RowLook, item: &ItemRow, index: usize, is_selected: bool) -> RowElement {
    let t = look.theme;
    let source = look.menu.item(&item.path);
    let color = if item.enabled {
        t.colors.text
    } else {
        t.colors.faint
    };
    let accelerator = source.and_then(|item| item.accelerator.clone());
    line(t, index, is_selected)
        .aria_label(item.breadcrumb())
        .text_color(color)
        .when(item.enabled, |row| row.cursor_pointer())
        .child(icon_cell(item_icon(look, item, source)))
        .child(label(
            t,
            item,
            index,
            source.is_some_and(|item| item.default),
        ))
        .when_some(accelerator.filter(|_| !item.opens_submenu), |row, keys| {
            row.child(trailing_text(t, keys))
        })
        .when(item.opens_submenu, |row| {
            row.child(icon(IconName::ChevronRight, t.spacing.icon, t.colors.muted))
        })
}

fn item_icon(
    look: &RowLook,
    item: &ItemRow,
    source: Option<&MenuItem>,
) -> Option<gpui_kit::AnyElement> {
    let t = look.theme;
    if let ItemId::Ferrite(action) = item.id {
        let color = if item.enabled {
            t.colors.accent
        } else {
            t.colors.faint
        };
        return Some(icon(ferrite_icon(action), t.spacing.icon, color).into_any_element());
    }
    if let Some(image) = look.icons.get(&item.path) {
        let image = img(ImageSource::Render(image)).size(px(SHELL_ICON));
        return Some(image.into_any_element());
    }
    source
        .filter(|item| item.checked)
        .map(|_| icon(IconName::Check, t.spacing.icon, t.colors.muted).into_any_element())
}

fn label(t: &Theme, item: &ItemRow, index: usize, is_default: bool) -> Div {
    let text = item.text.as_str();
    let cell = div()
        .flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .when(is_default, |cell| cell.font_weight(FontWeight::BOLD))
        .when(!item.crumb.is_empty(), |cell| {
            let crumb = format!("{} › ", item.crumb.join(" › "));
            let crumb = div()
                .flex_none()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(crumb);
            cell.child(crumb)
        });
    let parts = item.matched.clone().and_then(|range| {
        Some((
            text.get(..range.start)?,
            text.get(range.clone())?,
            text.get(range.end..)?,
        ))
    });
    match parts {
        Some((before, found, after)) => cell
            .child(span(before))
            .child(found_span(t, index, found))
            .child(span(after).min_w_0().text_ellipsis()),
        None => cell.child(span(text).min_w_0().text_ellipsis()),
    }
}

fn span(text: &str) -> Div {
    div().flex_none().child(SharedString::from(text.to_owned()))
}

fn found_span(t: &Theme, index: usize, found: &str) -> RowElement {
    div()
        .id(("menu-match", index))
        .test_support()
        .aria_label(found.to_owned())
        .flex_none()
        .text_color(t.colors.accent_text)
        .font_weight(FontWeight::BOLD)
        .underline()
        .child(found.to_owned())
}

fn trailing_text(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.tiny)
        .font_family(t.typography.numbers_font.clone())
        .font_features(t.typography.numbers_font_features.clone())
        .text_color(t.colors.muted)
        .child(text.into())
}

pub(super) fn status_row(
    t: &Theme,
    index: usize,
    text: impl Into<SharedString>,
    color: Hsla,
) -> RowElement {
    let text = text.into();
    base(index)
        .role(Role::MenuItem)
        .aria_label(text.clone())
        .gap(t.spacing.md)
        .h(px(ROW))
        .px(t.spacing.sm)
        .text_color(color)
        .child(icon_cell(None))
        .child(div().flex_1().child(text))
}

pub(super) fn retry_row(t: &Theme, index: usize, is_selected: bool) -> RowElement {
    let text = tr("context-menu-retry");
    line(t, index, is_selected)
        .aria_label(text.clone())
        .cursor_pointer()
        .text_color(t.colors.text)
        .child(icon_cell(Some(
            icon(IconName::RotateCw, t.spacing.icon, t.colors.accent).into_any_element(),
        )))
        .child(div().flex_1().child(text))
}
