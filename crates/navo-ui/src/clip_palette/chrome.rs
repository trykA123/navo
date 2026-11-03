use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Role, SharedString, StatefulInteractiveElement, Styled, WindowControlArea, div, px,
};
use navo_l10n::{t, tr};

use super::rows::RowElement;
use super::view::{ClipPalette, Place};
use super::{ClipKind, ClipMode, ClipRow};
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

const INPUT_ROW: f32 = 44.;
const FIELD: f32 = 32.;
const TABS: f32 = 28.;
const FOOTER: f32 = 28.;
const ACTIONS: f32 = 22.;
const UNDERLINE: f32 = 2.;

fn mode_label(mode: ClipMode) -> String {
    tr(match mode {
        ClipMode::All => "clip-mode-all",
        ClipMode::Recent => "clip-mode-recent",
        ClipMode::Saved => "clip-mode-saved",
        ClipMode::Secrets => "clip-mode-secrets",
        ClipMode::Notes => "clip-mode-notes",
    })
}

pub(super) fn enter_label(row: &ClipRow, place: Place) -> String {
    tr(match (place, row.kind) {
        (_, ClipKind::Note) => "clip-act-edit-note",
        (Place::Window, ClipKind::Secret) => "clip-act-copy-hidden",
        (Place::Window, ClipKind::Path) => "clip-act-go",
        (Place::Window, _) => "clip-act-copy",
        (Place::Anywhere, ClipKind::Secret) => "clip-act-paste-hidden",
        (Place::Anywhere, _) => "clip-act-paste",
    })
}

/// A key cap next to the word that says what it does.
pub(super) fn hint(t: &Theme, id: &'static str, key: &str, label: String) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .child(
            div()
                .text_color(t.colors.text)
                .font_weight(FontWeight::MEDIUM)
                .child(label),
        )
        .children(Kbd::new(id, key))
}

impl ClipPalette {
    /// Lets the pop-up be dragged by this part of its header; the search field stays clickable.
    fn drag_area(&self, area: Div) -> Div {
        if self.place == Place::Anywhere {
            area.window_control_area(WindowControlArea::Drag)
        } else {
            area
        }
    }

    fn drag_handle(&self, content: impl IntoElement, t: &Theme) -> Div {
        self.drag_area(div().flex().items_center().h_full().pl(t.spacing.xl))
            .child(content)
    }

    pub(super) fn input_row(&self, t: &Theme) -> Div {
        let input = Input::new(&self.field)
            .id("clip-palette-input")
            .aria_label(tr("clip-search-label"))
            .appearance(false)
            .h(px(FIELD))
            .text_size(t.typography.body);
        let key = self
            .key_cap
            .as_ref()
            .and_then(|cap| Kbd::new("clip-palette-summon-key", cap));
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .h(px(INPUT_ROW))
            .pr(t.spacing.md)
            .child(self.drag_handle(icon(IconName::Search, t.spacing.icon, t.colors.muted), t))
            .child(div().flex_1().min_w_0().child(input))
            .children(key)
    }

    fn tab(
        &self,
        t: &Theme,
        (index, mode): (usize, ClipMode),
        cx: &mut Context<Self>,
    ) -> RowElement {
        let is_current = mode == self.state.mode();
        let label = SharedString::from(mode_label(mode));
        let count = self.state.count_of(mode);
        div()
            .id(("clip-palette-mode", index))
            .role(Role::Tab)
            .aria_label(label.clone())
            .aria_selected(is_current)
            .test_support()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xxs)
            .cursor_pointer()
            .text_size(t.typography.small)
            .font_weight(if is_current {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .text_color(if is_current {
                t.colors.text
            } else {
                t.colors.muted
            })
            .child(label)
            .children(count.map(|count| {
                div()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(t.colors.muted)
                    .child(count.to_string())
            }))
            .when(is_current, |tab| {
                tab.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(UNDERLINE))
                        .rounded(t.radii.sm)
                        .bg(t.colors.accent),
                )
            })
            .on_click(cx.listener(move |palette, _, _, cx| palette.pick_mode(mode, cx)))
    }

    /// Text tabs with counts for the modes the hooks offer, and the Tab key cap.
    pub(super) fn tabs(
        &self,
        t: &Theme,
        is_docked: bool,
        cx: &mut Context<Self>,
    ) -> Option<RowElement> {
        let modes = self.state.modes().to_vec();
        if modes.is_empty() {
            return None;
        }
        let has_cycle = modes.len() > 1 && !is_docked;
        let tabs: Vec<_> = modes
            .into_iter()
            .enumerate()
            .map(|tab| self.tab(t, tab, cx))
            .collect();
        Some(
            div()
                .id("clip-palette-modes")
                .role(Role::TabList)
                .aria_label(tr("clip-modes-label"))
                .test_support()
                .flex()
                .flex_none()
                .items_stretch()
                .gap(t.spacing.xl)
                .h(px(TABS))
                .px(t.spacing.xl)
                .border_b_1()
                .border_color(t.colors.line)
                .children(tabs)
                .child(self.drag_area(div().flex_1()))
                .when(has_cycle, |bar| {
                    bar.child(
                        div()
                            .flex()
                            .items_center()
                            .children(Kbd::new("clip-palette-tab-key", "tab")),
                    )
                }),
        )
    }

    /// The hooks' notice, then the first-open line.
    pub(super) fn notices(&self, t: &Theme) -> Vec<RowElement> {
        [self.notice(t), self.intro_line(t)]
            .into_iter()
            .flatten()
            .collect()
    }

    fn notice(&self, t: &Theme) -> Option<RowElement> {
        let text = SharedString::from(self.state.results().notice.clone()?);
        Some(note_row(t, "clip-palette-notice", text, false))
    }

    /// The line shown on the first open: what is kept and where to turn it off.
    fn intro_line(&self, t: &Theme) -> Option<RowElement> {
        let text = SharedString::from(tr("clip-intro"));
        self.intro
            .then(|| note_row(t, "clip-palette-intro", text, true))
    }

    fn status(&self, t: &Theme, cx: &App) -> Div {
        let results = self.state.results();
        let line = div().flex().items_center().gap(t.spacing.sm).min_w_0();
        if !self.query(cx).is_empty() {
            return line.child(t!("clip-matches", count = results.rows.len()));
        }
        match self.state.shown() {
            ClipMode::Secrets => line
                .child(icon(IconName::Shield, t.typography.small, t.colors.muted))
                .child(tr("clip-vault-line")),
            ClipMode::All | ClipMode::Recent => line.child(t!("clip-kept", count = results.kept)),
            _ => line.child(t!("clip-items", count = self.state.item_count())),
        }
    }

    /// The status, the Enter label and the Actions button, which opens the actions menu.
    pub(super) fn footer(&self, t: &Theme, cx: &mut Context<Self>) -> RowElement {
        if let Some(ask) = self.repoint_footer(t, cx) {
            return ask;
        }
        let enter = self.state.selected_row().map(|row| {
            let label = enter_label(row, self.place);
            hint(t, "clip-palette-enter-key", "enter", label).into_any_element()
        });
        let actions = footer_button(t, "clip-palette-actions", tr("clip-act-actions"), "ctrl-k")
            .on_click(cx.listener(|palette, _, _, cx| palette.toggle_actions(cx)));
        let keys = enter.into_iter().chain([actions.into_any_element()]);
        footer_bar(t, self.status(t, cx), keys.collect())
    }
}

/// The footer: a status on the left, then the hints and buttons.
pub(super) fn footer_bar(t: &Theme, status: Div, keys: Vec<AnyElement>) -> RowElement {
    div()
        .id("clip-palette-footer")
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xl)
        .h(px(FOOTER))
        .pl(t.spacing.xl)
        .pr(t.spacing.md)
        .border_t_1()
        .border_color(t.colors.line)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .whitespace_nowrap()
        .child(div().flex_1().min_w_0().truncate().child(status))
        .children(keys)
}

/// A footer button with its key cap; the caller adds the click.
pub(super) fn footer_button(t: &Theme, id: &'static str, label: String, key: &str) -> RowElement {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .h(px(ACTIONS))
        .px(t.spacing.xxs)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .hover(|button| button.bg(t.colors.hover))
        .child(label)
        .children(Kbd::new((id, 1usize), key))
}

/// A calm band under the tabs. The icon sits on the first text line; `wraps` lets the text
/// take more lines instead of cutting it.
fn note_row(t: &Theme, id: &'static str, text: SharedString, wraps: bool) -> RowElement {
    div()
        .id(id)
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_start()
        .gap(t.spacing.md)
        .px(t.spacing.xl)
        .py(t.spacing.sm)
        .border_b_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .h(t.typography.line_height)
                .child(icon(IconName::Info, t.typography.small, t.colors.muted)),
        )
        .child(
            div()
                .min_w_0()
                .when(!wraps, |text| text.truncate())
                .child(text),
        )
}
