//! The Topbar's drawing: breadcrumbs, the filter field, the action pills and the width probe,
//! and its `Render` impl. Split out of topbar_status.rs, which keeps its state and actions.

use gpui_kit::base::{TestSupportExt as _, TestTextExt as _};
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::{Disableable as _, Sizable as _};
use gpui_kit::{Hsla, ReadGlobal, Role};
use sidewise_kit::prelude::*;
use sidewise_kit::tokens;

use crate::theme::SidewiseTheme;
use crate::theme_data::palettes::{DIMS, ModeTokens};

use super::mode::{Action, PILLS, Pill};
use super::{AccountConnection, CONTEXT, FocusSearch, PILL_H, SyncPlan, TOP_H, Topbar, ViewMenu};

impl Topbar {
    /// Builds one single-line ellipsized breadcrumb span bounded inside its flex column.
    pub(super) fn crumb_part(text: SharedString, color: Hsla) -> impl IntoElement {
        div()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_color(color)
            .child(text.clone().test_text(text))
    }

    /// The mode/screen crumbs: single-line ellipsized title (plus detail at normal width).
    pub(super) fn crumb_view(&self, m: &ModeTokens) -> impl IntoElement {
        let label: SharedString = if self.detail.is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.title, self.detail).into()
        };
        div()
            .id("top-crumbs")
            .role(Role::Label)
            .aria_label(label)
            .test_support()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .overflow_hidden()
            .child(Self::crumb_part(self.title.clone(), m.text))
            .when(!self.compact && !self.detail.is_empty(), |crumbs| {
                crumbs.child(Self::crumb_part(self.detail.clone(), m.muted))
            })
    }

    /// The inline `/` search filter backed by the kit `Input` primitive with disabled support.
    pub(super) fn search_view(&self, m: &ModeTokens, cx: &mut Context<Self>) -> AnyElement {
        let tokens = tokens::current(cx);
        let input = Input::new(&self.filter_input)
            .id("top-search")
            .aria_label("Search files")
            .small()
            .font_family(tokens.typography.mono_font.clone())
            .text_size(tokens.typography.small)
            .disabled(!self.is_available(Action::Search));
        div()
            .flex()
            .items_center()
            .gap_1()
            .flex_initial()
            .flex_shrink(1.0)
            .w(px(250.))
            .min_w(px(80.))
            .h(px(PILL_H))
            .pl_2()
            .rounded(DIMS.btn_r)
            .bg(m.surface)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.focus_search(window, cx)),
            )
            .child(div().text_color(m.muted).child("/".test_text("/")))
            .child(div().flex_1().min_w_0().overflow_hidden().child(input))
            .into_any_element()
    }

    /// The optional leading swatch or status dot for the Theme and Accounts buttons.
    pub(super) fn pill_indicator(&self, action: Action, m: &ModeTokens) -> Option<AnyElement> {
        match action {
            Action::Theme => Some(
                div()
                    .id("top-theme-swatch")
                    .w(px(8.))
                    .h(px(8.))
                    .rounded_full()
                    .flex_none()
                    .bg(m.accent)
                    .border_1()
                    .border_color(m.sep_strong)
                    .test_support()
                    .into_any_element(),
            ),
            Action::Accounts => {
                let (color, status) = match self.connection {
                    AccountConnection::Connected => (m.accent, "Account connected"),
                    AccountConnection::Disconnected => (m.faint, "Account disconnected"),
                };
                Some(
                    div()
                        .id("top-accounts-status")
                        .role(Role::Status)
                        .aria_label(status)
                        .aria_description(status)
                        .w(px(8.))
                        .h(px(8.))
                        .rounded_full()
                        .flex_none()
                        .bg(color)
                        .test_support()
                        .into_any_element(),
                )
            }
            _ => None,
        }
    }

    /// One action pill as a kit button backed by the Sidewise theme tokens.
    pub(super) fn pill_view(
        &self,
        pill: &Pill,
        m: &ModeTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action = pill.action;
        let enabled = self.is_available(action);
        Button::new(pill.id)
            .small()
            .disabled(!enabled)
            .accessibility_label(pill.label)
            .px_2()
            .h(px(PILL_H))
            .rounded(DIMS.btn_r)
            .on_click(cx.listener(move |this, _, _, cx| this.fire(action, cx)))
            .when_some(self.pill_indicator(action, m), |row, indicator| {
                row.child(indicator)
            })
            .child(pill.label.test_text(pill.label))
            .when_some(
                pill.hint.filter(|_| enabled && !self.compact),
                |row, hint| row.child(div().text_color(m.faint).child(hint.test_text(hint))),
            )
            .into_any_element()
    }
    /// Returns a zero-hit overlay canvas that measures own bar width without taking flex space.
    pub(super) fn width_probe(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        gpui_kit::canvas(
            move |bounds, _, cx| {
                let _ = this.update(cx, |bar, cx| bar.record_width(bounds.size.width, cx));
            },
            |_, (), _, _| {},
        )
        .absolute()
        .inset_0()
    }
}

impl Render for Topbar {
    /// The 52 px bar on `--bg2` with a `--sep` hairline, its text 16 px off the sides. The
    /// crumbs and the filter take the shrinking space; the pills keep their size.
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = *SidewiseTheme::global(cx);
        let m = if side.mode.is_dark() {
            side.spec.dark
        } else {
            side.spec.light
        };
        let crumb = self.crumb_view(&m);
        let search = self
            .mode
            .shows(Action::Search)
            .then(|| self.search_view(&m, cx));
        let pills: Vec<AnyElement> = PILLS
            .iter()
            .filter(|pill| self.mode.shows(pill.action))
            .map(|pill| self.with_menu(pill.action, self.pill_view(pill, &m, cx)))
            .collect();
        div()
            .relative()
            .key_context(CONTEXT)
            .tab_group()
            .track_focus(&self.handle)
            .on_action(cx.listener(|this, _: &ViewMenu, _, cx| this.fire(Action::View, cx)))
            .on_action(cx.listener(|this, _: &SyncPlan, _, cx| this.fire(Action::Sync, cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.focus_search(window, cx);
            }))
            .flex()
            .flex_none()
            .items_center()
            .when(!self.compact, |bar| bar.gap_3())
            .when(self.compact, |bar| bar.gap_1())
            .h(px(TOP_H))
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .bg(m.surface2)
            .border_b_1()
            .border_color(m.sep)
            .px(DIMS.inset_x)
            .child(self.width_probe(cx))
            .child(crumb)
            .child(div().flex_1().min_w_0())
            .when_some(search, |bar, search| bar.child(search))
            .children(pills)
    }
}
