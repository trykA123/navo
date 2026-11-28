//! The window title bar: the search field and the trailing "View" button, put together into
//! the `NavoTitleBar` drawn at the top of every window.

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::Icon;
use gpui_kit::{FontWeight, Hsla, MouseButton, Role, canvas};

use super::keys::OpenViewMenu;
use super::{NavoWindow, UpdateView};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, Tooltip, icon};
use crate::settings::{Field, WindowParts};
use crate::title_bar::{HeaderSearch, NavoTitleBar};

const PILL_HEIGHT: f32 = 26.;
const PILL_ICON: f32 = 13.;
const DISMISS_SIZE: f32 = 20.;
const DISMISS_ICON: f32 = 12.;

struct PillStyle {
    glyph: IconName,
    label: SharedString,
    tooltip: SharedString,
    bg: Hsla,
    hover_bg: Option<Hsla>,
    border: Hsla,
    icon_color: Hsla,
    text_color: Hsla,
    is_ready: bool,
}

fn dismiss_button(cx: &mut Context<NavoWindow>) -> impl IntoElement {
    let t = crate::tokens::current(cx);
    let label = SharedString::from(navo_l10n::tr("title-update-dismiss"));
    let tip = label.clone();
    let (hover_bg, hover_fg, rest_fg) = (t.colors.hover, t.colors.text, t.colors.muted);
    let (size, icon_size, radius) = (px(DISMISS_SIZE), px(DISMISS_ICON), t.radii.full);
    div()
        .id("title-update-pill-dismiss")
        .role(Role::Button)
        .aria_label(label)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(size)
        .rounded(radius)
        .text_color(rest_fg)
        .cursor_pointer()
        .hover(move |btn| btn.bg(hover_bg).text_color(hover_fg))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|view: &mut NavoWindow, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.dismiss_update_pill(cx);
        }))
        .child(Icon::new(IconName::Close).size(icon_size).flex_none())
}

/// The title-bar "View" button. `compare_open` is true while the compare screen is up: the
/// click then opens the compare View menu with "Wrap lines" directly (W8n), whatever holds
/// focus, else the window's, as today.
fn view_button(compare_open: bool, cx: &mut Context<NavoWindow>) -> impl IntoElement {
    let open = cx.listener(move |view: &mut NavoWindow, _: &ClickEvent, window, cx| {
        if compare_open {
            view.host_open_look_menu(window, cx);
        } else {
            window.dispatch_action(Box::new(OpenViewMenu), cx);
        }
    });
    Button::new("window-view", "View")
        .variant(ButtonVariant::Ghost)
        .tooltip("View menu (Alt+V)")
        .on_click(open)
}

impl NavoWindow {
    fn search_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let anchor = self.command_center.anchor.clone();
        let open = cx.listener(|view: &mut Self, _: &ClickEvent, window, cx| {
            if view.command_center_view().is_none() {
                view.toggle_command_center(window, cx);
            }
        });
        div()
            .id("title-bar-search-open")
            .relative()
            .flex()
            .min_w_0()
            .cursor_pointer()
            .on_click(open)
            .child(HeaderSearch::new())
            .child(
                canvas(
                    move |bounds, _, _| anchor.set(Some(bounds)),
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }

    pub(super) fn search_anchor(&self, cx: &App) -> Option<Bounds<Pixels>> {
        let is_ribbon = self.workspace_ribbon(cx).is_some();
        self.command_center.anchor.get().filter(|_| !is_ribbon)
    }

    fn pill_style(&self, cx: &App) -> Option<PillStyle> {
        if self.identity.install == crate::app_menu::Install::Portable
            || self.settings.updates.is_pill_dismissed()
        {
            return None;
        }
        let t = crate::tokens::current(cx);
        match &self.settings.updates.view {
            UpdateView::Downloading { percent, .. } => Some(PillStyle {
                glyph: IconName::LoaderCircle,
                label: navo_l10n::t!("title-update-downloading", percent = i64::from(*percent))
                    .into(),
                tooltip: navo_l10n::tr("title-update-open-tip").into(),
                bg: t.surfaces.card,
                hover_bg: Some(t.colors.hover),
                border: t.colors.line,
                icon_color: t.colors.accent_text,
                text_color: t.colors.text,
                is_ready: false,
            }),
            UpdateView::Ready { version } => {
                let tip_key = if self.restart_available() {
                    "title-update-ready-tip"
                } else {
                    "title-update-open-tip"
                };
                Some(PillStyle {
                    glyph: IconName::RotateCw,
                    label: navo_l10n::t!("title-update-ready", version = version.as_ref()).into(),
                    tooltip: navo_l10n::tr(tip_key).into(),
                    bg: t.colors.selection,
                    hover_bg: None,
                    border: t.colors.accent,
                    icon_color: t.colors.accent_text,
                    text_color: t.colors.on_selection,
                    is_ready: true,
                })
            }
            UpdateView::Failed(_) => Some(PillStyle {
                glyph: IconName::TriangleAlert,
                label: navo_l10n::tr("title-update-failed").into(),
                tooltip: navo_l10n::tr("title-update-open-tip").into(),
                bg: t.surfaces.field,
                hover_bg: Some(t.colors.hover),
                border: t.colors.mark,
                icon_color: t.colors.mark,
                text_color: t.colors.text,
                is_ready: false,
            }),
            UpdateView::Hidden
            | UpdateView::Off
            | UpdateView::Idle
            | UpdateView::Checking
            | UpdateView::UpToDate => None,
        }
    }

    fn title_update_pill(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let style = self.pill_style(cx)?;
        let (gap, pl, pr, radius, text_size) = {
            let t = crate::tokens::current(cx);
            (
                t.spacing.xs,
                t.spacing.md,
                t.spacing.xxs,
                t.radii.full,
                t.typography.tiny,
            )
        };
        let is_ready = style.is_ready;
        let on_open = cx.listener(move |view: &mut Self, _: &ClickEvent, window, cx| {
            if is_ready && view.restart_available() {
                view.request_restart(window, cx);
            } else {
                view.open_settings_at(Field::CheckUpdates, window, cx);
            }
        });
        let tip = style.tooltip;
        let hover_bg = style.hover_bg;
        Some(
            div()
                .id("title-update-pill")
                .role(Role::Button)
                .aria_label(style.label.clone())
                .test_support()
                .flex()
                .flex_none()
                .items_center()
                .gap(gap)
                .h(px(PILL_HEIGHT))
                .pl(pl)
                .pr(pr)
                .rounded(radius)
                .border_1()
                .border_color(style.border)
                .bg(style.bg)
                .text_size(text_size)
                .font_weight(FontWeight::MEDIUM)
                .text_color(style.text_color)
                .cursor_pointer()
                .hover(move |pill| match hover_bg {
                    Some(bg) => pill.bg(bg),
                    None => pill.opacity(0.9),
                })
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .on_click(on_open)
                .child(icon(style.glyph, px(PILL_ICON), style.icon_color))
                .child(div().whitespace_nowrap().child(style.label))
                .child(dismiss_button(cx))
                .into_any_element(),
        )
    }

    pub(super) fn title_bar(&self, cx: &mut Context<Self>) -> NavoTitleBar {
        let this = cx.weak_entity();
        let toggle = cx.listener(|view: &mut Self, _: &ClickEvent, window, cx| {
            view.toggle_app_menu(window, cx);
        });
        let bar = NavoTitleBar::new(navo_core::brand::NAME)
            .on_mica()
            .app_button(self.app_menu.is_some(), toggle);
        let bar = if let Some(ribbon) = self.workspace_ribbon(cx) {
            self.command_center.anchor.set(None);
            bar.header(ribbon)
                .header_inset(crate::tokens::current(cx).spacing.xxl * 8.)
        } else if WindowParts::of(&self.values).title_search {
            bar.header(self.search_field(cx))
        } else {
            self.command_center.anchor.set(None);
            bar
        };
        let bar = match self.title_update_pill(cx) {
            Some(pill) => bar.trailing(pill),
            None => bar,
        };
        let bar = match self.title_terminal_button(cx) {
            Some(button) => bar.trailing(button),
            None => bar,
        };
        let bar = if self.can_open_window_menu() {
            bar.trailing(view_button(self.host_screen_open(cx), cx))
        } else {
            bar
        };
        bar.on_close(move |window, cx| {
            if this
                .update(cx, |view, cx| view.close_or_hide(window, cx))
                .unwrap_or(true)
            {
                cx.quit();
            }
        })
    }
}
