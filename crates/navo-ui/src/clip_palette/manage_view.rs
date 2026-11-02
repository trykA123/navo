//! Drawing of the library view: the kinds, the list of one kind, and the footer.

use std::rc::Rc;

use gpui_kit::base::{Scrollbar, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Div, FontWeight, Hsla, InteractiveElement, IntoElement,
    MouseMoveEvent, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled,
    WeakEntity, Window, div, px,
};
use navo_l10n::{t, tr};

use super::ClipRow;
use super::chrome::{footer_bar, footer_button};
use super::manage::Manage;
use super::manage_kinds::{KINDS, Kind};
use super::rows::{self, Place, ROW, RowElement};
use super::view::ClipPalette;
use crate::primitives::{IconName, VirtualList, icon};
use crate::tokens::Theme;

const NAV_WIDTH: f32 = 168.;
/// A kind that cannot be listed shows this where its count would be.
const NO_COUNT: &str = "–";

fn kind_icon(t: &Theme, kind: Kind) -> (IconName, Hsla) {
    match kind {
        Kind::History => (IconName::Clipboard, t.colors.muted),
        Kind::Snippets => (IconName::FileText, t.colors.muted),
        Kind::Commands => (IconName::SquareTerminal, t.colors.muted),
        Kind::Paths => (IconName::Folder, t.colors.folder),
        Kind::Secrets => (IconName::Lock, t.colors.warn),
        Kind::Notes => (IconName::FilePen, t.colors.muted),
    }
}

fn count_label(t: &Theme, count: Option<usize>, is_current: bool) -> Div {
    let color = if is_current {
        t.colors.on_selection
    } else {
        t.colors.muted
    };
    div()
        .flex_none()
        .text_size(t.typography.small)
        .font_weight(FontWeight::NORMAL)
        .font_features(t.typography.numbers_font_features.clone())
        .text_color(color)
        .child(count.map_or_else(|| NO_COUNT.to_owned(), |count| count.to_string()))
}

/// One kind: icon, name and count. A kind the hooks cannot list is dimmed and has no click.
fn kind_item(t: &Theme, manage: &Manage, (index, kind): (usize, Kind)) -> RowElement {
    let (is_current, is_listed) = (kind == manage.kind(), manage.is_listed(kind));
    let theme = t.selected_content(is_current);
    let t = &*theme;
    let label = SharedString::from(kind.label());
    let (name, color) = kind_icon(t, kind);
    let spoken = if is_listed {
        label.clone()
    } else {
        t!("clip-manage-unavailable", kind = label.to_string()).into()
    };
    div()
        .id(("clip-manage-kind", index))
        .role(Role::Tab)
        .aria_label(spoken)
        .aria_selected(is_current)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .h(px(ROW))
        .px(t.spacing.md)
        .rounded(t.radii.md)
        .text_color(if is_listed {
            t.colors.text
        } else {
            t.colors.muted
        })
        .font_weight(if is_current {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .when(is_current, |item| {
            item.bg(t.colors.selection)
                .text_color(t.colors.on_selection)
        })
        .when(is_listed && !is_current, |item| {
            item.cursor_pointer().hover(|item| item.bg(t.colors.hover))
        })
        .child(icon(name, t.spacing.icon, color))
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(count_label(t, manage.count_of(kind), is_current))
}

impl ClipPalette {
    /// The kinds with their counts: a column beside the list, or a strip above it when docked.
    fn kinds(&self, t: &Theme, manage: &Manage, cx: &mut Context<Self>) -> RowElement {
        let items: Vec<RowElement> = KINDS
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                let item = kind_item(t, manage, (index, kind));
                if !manage.is_listed(kind) {
                    return item;
                }
                item.on_click(cx.listener(move |palette, _, _, cx| palette.pick_kind(kind, cx)))
            })
            .collect();
        div()
            .id("clip-manage-kinds")
            .role(Role::TabList)
            .aria_label(tr("clip-manage-kinds-label"))
            .test_support()
            .flex_none()
            .p(t.spacing.xs)
            .gap(t.spacing.xxs)
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .children(items)
    }

    fn manage_rows(&self, manage: &Manage, cx: &mut Context<Self>) -> AnyElement {
        let loaded = manage.is_loaded().then(|| manage.kind().empty_mode());
        if manage.rows().is_empty() {
            return self.empty_for(loaded, crate::tokens::current(cx), cx);
        }
        let selected = manage.selected();
        let sheet = Sheet {
            rows: manage.rows().clone(),
            selected,
            now_ms: self.now_ms,
            view: cx.entity().downgrade(),
        };
        let len = sheet.rows.len();
        VirtualList::new(
            "clip-manage-list",
            len,
            &manage.scroll,
            move |range, _, cx| sheet.paint(range, cx),
        )
        .row_height(px(ROW))
        .label(tr("clip-results-label"))
        .describe_rows(move |index| crate::primitives::RowSemantics {
            label: None,
            is_selected: Some(index == selected),
            is_active_descendant: index == selected,
        })
        .into_any_element()
    }

    /// The library: the kinds and the rows of the current one.
    pub(super) fn manage_body(&self, t: &Theme, is_docked: bool, cx: &mut Context<Self>) -> Div {
        let Some(manage) = self.manage.as_ref() else {
            return div();
        };
        let list = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .p(t.spacing.xs)
            .child(self.manage_rows(manage, cx));
        let nav = self.kinds(t, manage, cx);
        let nav = if is_docked {
            nav.grid().grid_cols(3).border_b_1()
        } else {
            nav.flex().flex_col().w(px(NAV_WIDTH)).border_r_1()
        };
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .when(is_docked, |body| body.flex_col())
            .child(nav)
            .child(list)
    }

    /// The count, then Delete for the selected row and the way back.
    pub(super) fn manage_footer(&self, t: &Theme, cx: &mut Context<Self>) -> RowElement {
        let Some(manage) = self.manage.as_ref() else {
            return footer_bar(t, div(), Vec::new());
        };
        let count = manage.rows().len();
        let status = if self.query(cx).is_empty() {
            t!("clip-items", count = count)
        } else {
            t!("clip-matches", count = count)
        };
        let delete = manage.selected_row().map(|_| {
            footer_button(t, "clip-manage-delete", tr("clip-act-delete"), "delete")
                .on_click(cx.listener(|palette, _, _, cx| palette.manage_delete(cx)))
                .into_any_element()
        });
        let back = footer_button(t, "clip-manage-back", tr("clip-manage-back"), "escape")
            .on_click(cx.listener(|palette, _, _, cx| palette.close_manage(cx)));
        let keys = delete
            .into_iter()
            .chain([back.into_any_element()])
            .collect();
        footer_bar(t, div().child(status), keys)
    }
}

/// Everything the list needs to draw a range of rows from a `'static` closure.
struct Sheet {
    rows: Rc<[ClipRow]>,
    selected: usize,
    now_ms: i64,
    view: WeakEntity<ClipPalette>,
}

impl Sheet {
    fn row(&self, index: usize, t: &Theme) -> AnyElement {
        let is_selected = index == self.selected;
        let place = Place {
            index,
            is_selected,
            now_ms: self.now_ms,
            gutter: Scrollbar::width(),
            shows_source: true,
        };
        let (moved, chosen) = (self.view.clone(), self.view.clone());
        let hover = move |_: &MouseMoveEvent, _: &mut Window, cx: &mut App| {
            let _ = moved.update(cx, |palette, cx| palette.manage_select(index, cx));
        };
        let click = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = chosen.update(cx, |palette, cx| palette.manage_select(index, cx));
        };
        rows::row(t, &self.rows[index], place)
            .when(!is_selected, |row| row.on_mouse_move(hover))
            .on_click(click)
            .into_any_element()
    }

    fn paint(&self, range: std::ops::Range<usize>, cx: &App) -> Vec<AnyElement> {
        let t = crate::tokens::current(cx);
        range.map(|index| self.row(index, t)).collect()
    }
}
