//! 36 px query row for the floating find-results window: token chips, toggles, run controls, and inline edit.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Focusable, MouseButton, Role};
use navo_core::find::FindQuery;
use navo_l10n::t;

use super::FindWindow;
use crate::command_center::ContentSource;
use crate::command_center::content_source::{ContentWalk, token_message};
use crate::prelude::*;
use crate::primitives::{
    Button, ButtonVariant, IconName, TextField, TextFieldState, Tooltip, icon, text_field_state,
};
use crate::tokens::{self, Theme};

/// Fixed logical height of the query row under the title bar.
const QUERY_ROW_HEIGHT: f32 = 36.;
/// Chip height inside the 36 px query row.
const CHIP_HEIGHT: f32 = 22.;
/// Close (`×`) glyph size inside each token chip.
const REMOVE_GLYPH_SIZE: f32 = 11.;
/// Upper width bound for a single token chip before its label truncates.
const CHIP_MAX_WIDTH: f32 = 160.;
/// Approximate horizontal width reserved for right-side toggles, action button, and padding.
const CONTROLS_RESERVE: f32 = 190.;
/// Approximate width per character when estimating token chip fit.
const CHAR_WIDTH: f32 = 7.0;
/// Horizontal padding + remove button + gap chrome per chip.
const CHIP_CHROME: f32 = 34.0;
/// Horizontal space reserved for the `+N` overflow badge when chips overflow.
const OVERFLOW_WIDTH: f32 = 40.0;

/// Active inline query editor bound to one tab id.
pub(super) struct QueryEdit {
    tab_id: u64,
    input: Entity<TextFieldState>,
}

impl QueryEdit {
    /// Tab id being edited.
    pub(super) fn tab_id(&self) -> u64 {
        self.tab_id
    }
}

/// Snapshot of one parsed token chip in the active tab.
struct QueryChip {
    index: usize,
    label: SharedString,
    message: Option<SharedString>,
}

/// Active tab state needed to render the query row.
struct QuerySnapshot {
    has_tab: bool,
    chips: Vec<QueryChip>,
    case: bool,
    regex: bool,
    running: bool,
}

impl FindWindow {
    /// Enters inline query editing for the active tab and focuses the text input.
    pub(super) fn start_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tab_id, text)) = self
            .book()
            .read(cx)
            .active()
            .map(|tab| (tab.id, tab.text.clone()))
        else {
            return;
        };
        let input = text_field_state(t!("find-query-placeholder"), window, cx);
        input.update(cx, |field, cx| {
            field.set_value(text, window, cx);
            field.select_all(window, cx);
        });
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.edit = Some(QueryEdit { tab_id, input });
        cx.notify();
    }

    /// Cancels inline query editing without modifying the tab's query, restoring results focus.
    pub(super) fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.edit.take().is_none() {
            return false;
        }
        self.focus(window, cx);
        cx.notify();
        true
    }

    /// Commits the inline editor's text, re-parses the query, starts a new run, and restores focus.
    pub(super) fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.edit.take() else {
            return false;
        };
        let text = edit.input.read(cx).value().to_string();
        let walk = self.walk(cx);
        self.book().update(cx, |book, cx| {
            let Some(tab) = book.active_mut() else {
                return;
            };
            tab.text = text;
            tab.start(walk);
            cx.notify();
        });
        self.focus(window, cx);
        cx.notify();
        true
    }

    /// Removes the token at `index`, reprints cleanly with `FindQuery::to_text`, and re-runs.
    pub(super) fn remove_token(&mut self, index: usize, cx: &mut Context<Self>) {
        let walk = self.walk(cx);
        self.book().update(cx, |book, cx| {
            let Some(tab) = book.active_mut() else {
                return;
            };
            if index >= tab.query.tokens.len() {
                return;
            }
            tab.text = remove_token_text(&tab.text, &tab.query, index);
            tab.start(walk);
            cx.notify();
        });
    }

    /// Toggles match-case on the active tab's query, reprints via `to_text`, and re-runs.
    pub(super) fn toggle_case(&mut self, cx: &mut Context<Self>) {
        let walk = self.walk(cx);
        self.book().update(cx, |book, cx| {
            let Some(tab) = book.active_mut() else {
                return;
            };
            let mut next = tab.query.clone();
            next.case = !next.case;
            tab.text = next.to_text();
            tab.start(walk);
            cx.notify();
        });
    }

    /// Toggles regex mode on the active tab's query, reprints via `to_text`, and re-runs.
    pub(super) fn toggle_regex(&mut self, cx: &mut Context<Self>) {
        let walk = self.walk(cx);
        self.book().update(cx, |book, cx| {
            let Some(tab) = book.active_mut() else {
                return;
            };
            let mut next = tab.query.clone();
            next.regex = !next.regex;
            let wanted = next.regex;
            let empty_text = next.text.is_empty();
            tab.text = next.to_text();
            tab.start(walk);
            if empty_text {
                tab.query.regex = wanted;
            }
            cx.notify();
        });
    }

    /// Re-runs the active tab's current query from scratch.
    pub(super) fn run_active(&mut self, cx: &mut Context<Self>) {
        let walk = self.walk(cx);
        self.book().update(cx, |book, cx| {
            let Some(tab) = book.active_mut() else {
                return;
            };
            tab.start(walk);
            cx.notify();
        });
    }

    /// Stops the active tab's live search run, returning whether one was running.
    pub(super) fn stop_active(&mut self, cx: &mut Context<Self>) -> bool {
        self.book().update(cx, |book, cx| {
            let Some(tab) = book.active_mut() else {
                return false;
            };
            if !tab.is_running() {
                return false;
            }
            tab.stop();
            cx.notify();
            true
        })
    }

    /// Builds the 36 px query row containing token chips (or inline editor), toggles, and run button.
    pub(super) fn query_row(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let theme = tokens::current(cx).clone();
        let snap = snapshot(self.book().read(cx));
        let available = (f32::from(window.viewport_size().width) - CONTROLS_RESERVE).max(0.0);

        let left = match &self.edit {
            Some(edit) => edit_field(&edit.input).into_any_element(),
            None => self
                .chip_strip(&snap, available, &theme, cx)
                .into_any_element(),
        };

        div()
            .id("find-query-row")
            .test_support()
            .h(px(QUERY_ROW_HEIGHT))
            .w_full()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme.spacing.sm)
            .px(theme.spacing.sm)
            .bg(theme.colors.surface)
            .border_b_1()
            .border_color(theme.colors.line)
            .child(left)
            .child(self.right_controls(&snap, &theme, cx))
    }

    fn chip_strip(
        &self,
        snap: &QuerySnapshot,
        available: f32,
        t: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let shown = fit_chip_count(&snap.chips, available);
        let hidden = snap.chips.len().saturating_sub(shown);
        let hidden_tip = hidden_tooltip(&snap.chips[shown..]);
        let chip_elements: Vec<_> = snap
            .chips
            .iter()
            .take(shown)
            .map(|chip| self.render_chip(chip, t, cx))
            .collect();

        div()
            .id("find-query-strip")
            .role(Role::Button)
            .aria_label(t!("find-query-strip-label"))
            .test_support()
            .flex_1()
            .min_w_0()
            .h(px(CHIP_HEIGHT + 4.))
            .flex()
            .items_center()
            .gap(t.spacing.xs)
            .overflow_hidden()
            .cursor_text()
            .on_click(cx.listener(|view, _, window, cx| view.start_edit(window, cx)))
            .when(snap.chips.is_empty(), |strip| {
                strip.child(
                    div()
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .truncate()
                        .child(t!("find-query-placeholder")),
                )
            })
            .children(chip_elements)
            .when(hidden > 0, |strip| {
                strip.child(self.overflow_badge(hidden, hidden_tip, t, cx))
            })
    }

    fn render_chip(&self, chip: &QueryChip, t: &Theme, cx: &Context<Self>) -> AnyElement {
        let is_bad = chip.message.is_some();
        let border = if is_bad { t.find.bad } else { t.colors.line };
        let fg = if is_bad { t.find.bad } else { t.colors.text };
        let index = chip.index;
        let remove_label = t!("find-query-remove-token", token = chip.label.as_ref());
        let hover = t.colors.hover;

        div()
            .id(("find-query-chip", index))
            .role(Role::Label)
            .aria_label(chip.label.clone())
            .when_some(chip.message.clone(), |el, msg| el.aria_description(msg))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .h(px(CHIP_HEIGHT))
            .max_w(px(CHIP_MAX_WIDTH))
            .pl(t.spacing.sm)
            .pr(t.spacing.xs)
            .rounded(t.radii.pill)
            .border_1()
            .border_color(border)
            .bg(t.surfaces.field)
            .text_color(fg)
            .text_size(t.typography.tiny)
            .when_some(chip.message.clone(), |el, msg| {
                el.tooltip(move |window, cx| Tooltip::new(msg.clone()).build(window, cx))
            })
            .child(div().min_w_0().truncate().child(chip.label.clone()))
            .child(
                div()
                    .id(("find-query-chip-remove", index))
                    .role(Role::Button)
                    .aria_label(remove_label)
                    .test_support()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(16.))
                    .rounded(t.radii.pill)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |view, _, _, cx| view.remove_token(index, cx)))
                    .child(icon(IconName::X, px(REMOVE_GLYPH_SIZE), fg)),
            )
            .into_any_element()
    }

    fn overflow_badge(
        &self,
        hidden: usize,
        tip: SharedString,
        t: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let count = i64::try_from(hidden).unwrap_or(i64::MAX);
        let label: SharedString = t!("find-query-more-tokens", count = count).into();
        let hover = t.colors.hover;
        div()
            .id("find-query-overflow")
            .role(Role::Button)
            .aria_label(label.clone())
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .h(px(CHIP_HEIGHT))
            .px(t.spacing.sm)
            .rounded(t.radii.pill)
            .border_1()
            .border_color(t.colors.line)
            .bg(t.surfaces.field)
            .text_color(t.colors.muted)
            .text_size(t.typography.tiny)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|view, _, window, cx| view.start_edit(window, cx)))
            .child(label)
    }

    fn right_controls(
        &self,
        snap: &QuerySnapshot,
        t: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let action_button = if snap.running {
            Button::new("find-query-stop", t!("find-query-stop"))
                .variant(ButtonVariant::Danger)
                .on_click(cx.listener(|view, _, _, cx| {
                    let _ = view.stop_active(cx);
                }))
        } else {
            Button::new("find-query-run", t!("find-query-run-again"))
                .variant(ButtonVariant::Secondary)
                .disabled(!snap.has_tab)
                .on_click(cx.listener(|view, _, _, cx| view.run_active(cx)))
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .child(toggle_button(
                "find-query-case",
                t!("find-query-case"),
                t!("cmd-find-results-toggle-case"),
                snap.case,
                t,
                cx.listener(|view, _, _, cx| view.toggle_case(cx)),
            ))
            .child(toggle_button(
                "find-query-regex",
                t!("find-query-regex"),
                t!("cmd-find-results-toggle-regex"),
                snap.regex,
                t,
                cx.listener(|view, _, _, cx| view.toggle_regex(cx)),
            ))
            .child(action_button)
    }

    fn walk(&self, cx: &App) -> ContentWalk {
        self.navo()
            .read_with(cx, |view, _| view.command_center.content_source.walk())
            .unwrap_or_else(|_| ContentSource::default().walk())
    }
}

fn edit_field(input: &Entity<TextFieldState>) -> impl IntoElement {
    div().flex_1().min_w_0().child(TextField::new(
        "find-query-input",
        input,
        t!("cmd-find-results-edit-query"),
    ))
}

fn toggle_button(
    id: &'static str,
    text: String,
    label: String,
    active: bool,
    t: &Theme,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (bg, border, fg) = if active {
        (t.colors.hover, t.colors.accent, t.colors.accent_text)
    } else {
        (t.surfaces.field, t.colors.line, t.colors.muted)
    };
    let tip: SharedString = label.clone().into();
    let hover = t.colors.hover;

    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(active)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(CHIP_HEIGHT))
        .px(t.spacing.xs)
        .rounded(t.radii.sm)
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.tiny)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(on_click)
        .child(text)
}

fn snapshot(book: &crate::find_results::book::FindBook) -> QuerySnapshot {
    let Some(tab) = book.active() else {
        return QuerySnapshot {
            has_tab: false,
            chips: Vec::new(),
            case: false,
            regex: false,
            running: false,
        };
    };
    let chips = tab
        .query
        .tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            let label = tab.text.get(token.span.clone())?;
            Some(QueryChip {
                index,
                label: label.to_owned().into(),
                message: token.message.as_ref().map(token_message),
            })
        })
        .collect();
    QuerySnapshot {
        has_tab: true,
        chips,
        case: tab.query.case,
        regex: tab.query.regex,
        running: tab.is_running(),
    }
}

fn remove_token_text(text: &str, query: &FindQuery, remove_index: usize) -> String {
    let remaining: Vec<&str> = query
        .tokens
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != remove_index)
        .filter_map(|(_, token)| text.get(token.span.clone()))
        .collect();
    FindQuery::parse(&remaining.join(" ")).to_text()
}

fn estimated_chip_width(chip: &QueryChip) -> f32 {
    let chars = chip.label.chars().count() as f32;
    (chars * CHAR_WIDTH + CHIP_CHROME).min(CHIP_MAX_WIDTH)
}

fn fit_chip_count(chips: &[QueryChip], available: f32) -> usize {
    if chips.is_empty() {
        return 0;
    }
    let total: f32 = chips.iter().map(estimated_chip_width).sum();
    if total <= available {
        return chips.len();
    }
    let budget = (available - OVERFLOW_WIDTH).max(0.0);
    let mut used = 0.0;
    let mut count = 0;
    for chip in chips {
        let width = estimated_chip_width(chip);
        if count > 0 && used + width > budget {
            break;
        }
        used += width;
        count += 1;
        if used > budget {
            break;
        }
    }
    count
}

fn hidden_tooltip(hidden: &[QueryChip]) -> SharedString {
    hidden
        .iter()
        .map(|chip| chip.label.as_ref())
        .collect::<Vec<_>>()
        .join(" ")
        .into()
}
