//! Title bar and interactive search tab strip for the floating find-results window.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{MouseButton, Role, WeakEntity, transparent_black};
use navo_l10n::t;

use super::super::NavoWindow;
use super::FindWindow;
use crate::find_results::book::FindBook;
use crate::prelude::*;
use crate::primitives::{IconButton, IconName, icon};
use crate::title_bar::NavoTitleBar;
use crate::tokens::{self, Theme};

/// Horizontal inset reserved for the brand mark on the left and window controls on the right.
const HEADER_INSET: f32 = 104.;
/// Height of the search tab strip inside the title bar.
const STRIP_HEIGHT: f32 = 36.;
/// Leading search / running glyph size inside each tab.
const TAB_GLYPH_SIZE: f32 = 13.;
/// Close (`×`) glyph size inside each tab.
const CLOSE_GLYPH_SIZE: f32 = 12.;
/// Upper width bound for a single search tab before its query text truncates.
const TAB_MAX_WIDTH: f32 = 180.;

/// Snapshot of one search tab needed to render its title-bar pill.
struct TabItem {
    id: u64,
    label: SharedString,
    hits: u32,
    running: bool,
    active: bool,
}

/// Shared handles passed to tab-strip mouse handlers.
#[derive(Clone)]
struct StripContext {
    book: Entity<FindBook>,
    navo: WeakEntity<NavoWindow>,
    navo_window: AnyWindowHandle,
}

impl FindWindow {
    /// Builds the `NavoTitleBar` hosting the search tab strip, pin toggle, and window controls.
    pub(super) fn title_bar(&self, cx: &App) -> NavoTitleBar {
        let pinned = self.is_pinned();
        let tabs = read_tabs(self.book().read(cx));
        let strip_ctx = StripContext {
            book: self.book().clone(),
            navo: self.navo().clone(),
            navo_window: self.navo_window(),
        };
        let min_navo = self.navo().clone();
        let min_window = self.navo_window();
        let close_navo = self.navo().clone();
        let close_window = self.navo_window();

        NavoTitleBar::new(t!("find-title-empty"))
            .menu_arrow(false)
            .header(tab_strip(tabs, strip_ctx, cx))
            .header_inset(px(HEADER_INSET))
            .trailing(pin_button(pinned, self.navo().clone()))
            .on_minimize(t!("find-window-minimize"), move |_, cx| {
                let _ = min_navo.update(cx, |view, cx| view.hide_find(min_window, cx));
            })
            .on_close(move |window, cx| {
                let _ = close_navo.update(cx, |view, cx| {
                    view.close_find_by_user(close_window, window, cx);
                });
            })
    }
}

fn read_tabs(book: &FindBook) -> Vec<TabItem> {
    let active_index = book.active_index();
    let empty_label: SharedString = t!("find-window-new-tab").into();
    book.tabs()
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let label = if tab.text.trim().is_empty() {
                empty_label.clone()
            } else {
                tab.text.clone().into()
            };
            TabItem {
                id: tab.id,
                label,
                hits: tab.store.hit_total(),
                running: tab.is_running(),
                active: index == active_index,
            }
        })
        .collect()
}

fn tab_strip(tabs: Vec<TabItem>, ctx: StripContext, cx: &App) -> impl IntoElement {
    let theme = tokens::current(cx);
    div()
        .id("find-tab-strip")
        .role(Role::TabList)
        .test_support()
        .h(px(STRIP_HEIGHT))
        .flex()
        .items_center()
        .gap(theme.spacing.xs)
        .min_w_0()
        .max_w_full()
        .children(tabs.into_iter().map(|item| tab_pill(item, &ctx, cx)))
        .child(add_tab_button(&ctx))
}

fn tab_pill(item: TabItem, ctx: &StripContext, cx: &App) -> impl IntoElement {
    let t = tokens::current(cx);
    let bg = if item.active {
        t.colors.surface
    } else {
        transparent_black()
    };
    let border = if item.active {
        t.colors.line
    } else {
        transparent_black()
    };
    let fg = if item.active {
        t.colors.text
    } else {
        t.colors.muted
    };
    let hover = t.colors.hover;
    let id = item.id;
    let select_book = ctx.book.clone();
    let mid_ctx = ctx.clone();

    div()
        .id(("find-tab", id))
        .role(Role::Tab)
        .aria_selected(item.active)
        .aria_label(item.label.clone())
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .h(t.spacing.tab_height)
        .px(t.spacing.sm)
        .min_w_0()
        .max_w(px(TAB_MAX_WIDTH))
        .rounded(t.radii.md)
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .text_size(t.typography.small)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(move |_, _, cx| {
            select_book.update(cx, |book, cx| {
                if book.select(id) {
                    cx.notify();
                }
            });
        })
        .on_mouse_down(MouseButton::Middle, move |_, _, cx| {
            cx.stop_propagation();
            close_tab(&mid_ctx, id, cx);
        })
        .child(tab_leading(id, item.running, fg))
        .child(div().min_w_0().flex_1().truncate().child(item.label))
        .when(item.hits > 0, |pill| {
            pill.child(hit_badge(id, item.hits, t))
        })
        .child(tab_close_button(id, ctx, t))
}

fn tab_leading(id: u64, running: bool, color: Hsla) -> impl IntoElement {
    let (element_id, glyph) = if running {
        (("find-tab-progress", id), IconName::LoaderCircle)
    } else {
        (("find-tab-glyph", id), IconName::Search)
    };
    div()
        .id(element_id)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .child(icon(glyph, px(TAB_GLYPH_SIZE), color))
}

fn hit_badge(id: u64, hits: u32, t: &Theme) -> impl IntoElement {
    div()
        .id(("find-tab-hits", id))
        .test_support()
        .flex_none()
        .px(t.spacing.xs)
        .rounded(t.radii.pill)
        .bg(t.colors.hover)
        .text_color(t.colors.muted)
        .font_family(t.typography.numbers_font.clone())
        .text_size(t.typography.tiny)
        .child(hits.to_string())
}

fn tab_close_button(id: u64, ctx: &StripContext, t: &Theme) -> impl IntoElement {
    let close_ctx = ctx.clone();
    let hover = t.colors.hover;
    div()
        .id(("find-tab-close", id))
        .role(Role::Button)
        .aria_label(t!("tabs-close"))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(t.spacing.icon + t.spacing.xs)
        .rounded(t.radii.pill)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| close_tab(&close_ctx, id, cx))
        .child(icon(IconName::X, px(CLOSE_GLYPH_SIZE), t.colors.muted))
}

fn add_tab_button(ctx: &StripContext) -> impl IntoElement {
    let add_ctx = ctx.clone();
    IconButton::new("find-tab-add", IconName::Plus, t!("find-window-add-tab")).on_click(
        move |_, _, cx| {
            let base = add_ctx
                .navo
                .read_with(cx, |view, _| {
                    view.browser.state().active_tab().location().clone()
                })
                .ok()
                .or_else(|| add_ctx.book.read(cx).active().map(|tab| tab.base.clone()));
            let Some(base) = base else {
                return;
            };
            add_ctx.book.update(cx, |book, cx| {
                book.add("", base);
                cx.notify();
            });
        },
    )
}

fn pin_button(pinned: bool, navo: WeakEntity<NavoWindow>) -> impl IntoElement {
    let label = if pinned {
        t!("find-window-unpin")
    } else {
        t!("find-window-pin")
    };
    IconButton::new("find-window-pin", IconName::Pin, label)
        .selected(pinned)
        .on_click(move |_, _, cx| {
            let _ = navo.update(cx, |view, cx| view.toggle_find_pin(cx));
        })
}

fn close_tab(ctx: &StripContext, id: u64, cx: &mut App) {
    let emptied = ctx.book.update(cx, |book, cx| {
        if !book.close(id) {
            return false;
        }
        cx.notify();
        book.is_empty()
    });
    if emptied {
        let _ = ctx
            .navo
            .update(cx, |view, cx| view.hide_find(ctx.navo_window, cx));
    }
}
