use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, Hsla, ImageSource, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, Stateful, StatefulInteractiveElement, Styled, div, img, px,
};
use navo_core::shell_menu::{ItemId, ItemRow, MenuItem, NavoAction, Row, ShellMenu};

use super::icons::{ShellIcons, navo_icon};
use super::place::{ROW, SEPARATOR};
use navo_l10n::tr;

use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

const ICON_COLUMN: f32 = 20.;
const SHELL_ICON: f32 = 16.;
const KEYCAP_HEIGHT: f32 = 18.;
const KEYCAP_PAD_X: f32 = 5.;
const DANGER_EDGE_ALPHA: f32 = 0.45;

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

fn icon_cell(child: Option<gpui_kit::AnyElement>) -> Div {
    div()
        .w(px(ICON_COLUMN))
        .flex_none()
        .flex()
        .justify_center()
        .children(child)
}

pub(super) fn render(look: &RowLook, row: &Row, index: usize, is_selected: bool) -> RowElement {
    let theme = look.theme.selected_content(is_selected);
    let look = &RowLook {
        theme: &theme,
        ..*look
    };
    let t = look.theme;
    match row {
        Row::Separator => base(index)
            .h(px(SEPARATOR))
            .px(t.spacing.sm)
            .child(div().h(px(1.)).w_full().bg(t.colors.line)),
        Row::Item(item) => item_row(look, item, index, is_selected),
    }
}

fn is_danger(item: &ItemRow) -> bool {
    match &item.id {
        ItemId::Navo(NavoAction::Delete) => true,
        ItemId::Shell {
            verb: Some(verb), ..
        } => verb.eq_ignore_ascii_case("delete"),
        _ => false,
    }
}

fn item_row(look: &RowLook, item: &ItemRow, index: usize, is_selected: bool) -> RowElement {
    let t = look.theme;
    let source = look.menu.item(&item.path);
    let danger = item.enabled && is_danger(item);
    let color = if !item.enabled {
        t.colors.faint
    } else if danger {
        t.colors.mark
    } else {
        t.colors.text
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
            row.child(keycap(t, index, keys.into(), item))
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
    let danger = item.enabled && is_danger(item);
    if let ItemId::Navo(action) = item.id {
        let color = if !item.enabled {
            t.colors.faint
        } else if danger {
            t.colors.mark
        } else {
            t.colors.accent
        };
        return Some(icon(navo_icon(action), t.spacing.icon, color).into_any_element());
    }
    if let Some(image) = look.icons.get(&item.path) {
        let image = img(ImageSource::Render(image)).size(px(SHELL_ICON));
        return Some(image.into_any_element());
    }
    let check_color = if danger {
        t.colors.mark
    } else {
        t.colors.muted
    };
    source
        .filter(|item| item.checked)
        .map(|_| icon(IconName::Check, t.spacing.icon, check_color).into_any_element())
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
            .child(found_span(t, index, found, item.enabled && is_danger(item)))
            .child(span(after).min_w_0().text_ellipsis()),
        None => cell.child(span(text).min_w_0().text_ellipsis()),
    }
}

fn span(text: &str) -> Div {
    div().flex_none().child(SharedString::from(text.to_owned()))
}

fn found_span(t: &Theme, index: usize, found: &str, danger: bool) -> RowElement {
    let color = if danger {
        t.colors.mark
    } else {
        t.colors.accent_text
    };
    div()
        .id(("menu-match", index))
        .test_support()
        .aria_label(found.to_owned())
        .flex_none()
        .text_color(color)
        .font_weight(FontWeight::BOLD)
        .underline()
        .child(found.to_owned())
}

fn keycap(t: &Theme, index: usize, keys: SharedString, item: &ItemRow) -> RowElement {
    let danger = item.enabled && is_danger(item);
    let text = if !item.enabled {
        t.colors.faint
    } else if danger {
        t.colors.mark
    } else {
        t.colors.muted
    };
    let edge = if danger && !t.is_contrast {
        t.colors.mark.opacity(DANGER_EDGE_ALPHA)
    } else {
        t.surfaces.keycap_edge
    };
    let label = if t.is_contrast {
        t.surfaces.keycap_label
    } else {
        text
    };
    div()
        .id(("menu-key", index))
        .role(Role::Label)
        .aria_label(keys.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .min_h(px(KEYCAP_HEIGHT))
        .px(px(KEYCAP_PAD_X))
        .rounded(t.radii.keycap)
        .border_1()
        .border_color(edge)
        .bg(t.surfaces.keycap_face)
        .shadow(t.shadows.keycap.clone())
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.key)
        .font_weight(FontWeight::MEDIUM)
        .text_color(label)
        .child(keys)
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
    let theme = t.selected_content(is_selected);
    let t = &theme;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row_with(id: ItemId) -> ItemRow {
        ItemRow {
            path: vec![0],
            id,
            text: "Item".to_owned(),
            crumb: Vec::new(),
            matched: None,
            enabled: true,
            hidden: false,
            opens_submenu: false,
        }
    }

    #[test]
    fn delete_navo_and_shell_delete_verb_are_danger_rows() {
        assert!(is_danger(&row_with(ItemId::Navo(NavoAction::Delete))));
        assert!(is_danger(&row_with(ItemId::Shell {
            command: 1,
            verb: Some("delete".to_owned()),
        })));
        assert!(is_danger(&row_with(ItemId::Shell {
            command: 2,
            verb: Some("Delete".to_owned()),
        })));
        assert!(!is_danger(&row_with(ItemId::Navo(NavoAction::Copy))));
        assert!(!is_danger(&row_with(ItemId::Shell {
            command: 3,
            verb: Some("copy".to_owned()),
        })));
    }
}
