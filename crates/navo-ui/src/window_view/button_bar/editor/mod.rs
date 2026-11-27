//! Button bar editor overlay layer, header, and responsive split/tab body.

mod actions;
mod form;
mod form_state;
mod get_more;
mod picker;
mod picker_grid;
mod strip;

pub(crate) use form_state::FormState;
pub(crate) use picker::PickerState;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Role, Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};
use navo_l10n::tr;

use actions::default_button_draft;

use super::ItemPath;
use super::state::{ButtonBarEditorState, EditorPane};
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::{self, Theme};
use crate::window_view::{NavoWindow, keys};

pub(super) const CONTEXT: &str = "ButtonBarEditor";

const NARROW_WIDTH: f32 = 820.;
const DIALOG_MAX_W: f32 = 1040.;
const DIALOG_MAX_H: f32 = 800.;
const DIALOG_MARGIN: f32 = 24.;
const HEADER_HEIGHT: f32 = 52.;

const MSG_LABEL: &str = "button-bar-editor-label";
const MSG_TITLE: &str = "button-bar-editor-title";
const MSG_UNDO: &str = "button-bar-editor-undo";
const MSG_DONE: &str = "button-bar-editor-done";
const MSG_TAB_BUTTON: &str = "button-bar-editor-tab-button";
const MSG_TAB_ICON: &str = "button-bar-editor-tab-icon";
const MSG_DEFAULT_BUTTON: &str = "button-bar-editor-default-button";

impl NavoWindow {
    /// Opens the button bar editor overlay, optionally selecting `select`.
    pub(in crate::window_view) fn open_button_bar_editor(
        &mut self,
        select: Option<ItemPath>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.viewer.is_some() || self.restart_applying() {
            return;
        }
        if self.settings.is_open() {
            self.toggle_settings(window, cx);
        }
        self.close_popups(window, cx);
        self.close_command_center(window, cx);
        if self.app_menu.is_some() {
            self.close_app_menu(window, cx);
        }
        self.drive_menu = None;
        self.path_edit = None;

        let initial = select.or_else(|| {
            (!self.button_bar.items.is_empty()).then_some(ItemPath {
                index: 0,
                child: None,
            })
        });
        let (focus, resync) = match &mut self.button_bar.editor {
            Some(editor) => {
                let changed = select.is_some() && editor.selected != select;
                if select.is_some() {
                    editor.selected = select;
                }
                (editor.focus.clone(), changed)
            }
            None => {
                let focus = cx.focus_handle();
                let form = FormState::new(window, cx);
                let picker = PickerState::new(window, cx);
                self.button_bar.editor = Some(ButtonBarEditorState {
                    selected: initial,
                    focus: focus.clone(),
                    narrow_pane: EditorPane::default(),
                    error: None,
                    form,
                    picker,
                });
                self.start_picker_load(window, cx);
                (focus, true)
            }
        };
        if resync {
            self.sync_form_from_selected(window, cx);
        }
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Closes the button bar editor overlay and returns focus to the active pane.
    pub(in crate::window_view) fn close_button_bar_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.button_bar.editor.take().is_some() {
            self.button_bar.picker_generation = self.button_bar.picker_generation.wrapping_add(1);
            self.button_bar.get_more_generation =
                self.button_bar.get_more_generation.wrapping_add(1);
            self.focus_core_pane(window, cx);
            cx.notify();
        }
    }

    /// Renders the button bar editor overlay layer when open and not covered by a viewer.
    pub(in crate::window_view) fn button_bar_editor_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.viewer.is_some() {
            return None;
        }
        let editor = self.button_bar.editor.as_ref()?;
        let t = tokens::current(cx).clone();
        let card = self.editor_card(editor, window, &t, cx);
        let scrim = div()
            .id("button-bar-editor-scrim")
            .test_support()
            .absolute()
            .inset_0()
            .p(px(DIALOG_MARGIN))
            .flex()
            .items_center()
            .justify_center()
            .bg(t.surfaces.scrim)
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| {
                    view.close_button_bar_editor(window, cx);
                }),
            )
            .child(card);
        Some(scrim.into_any_element())
    }

    fn editor_card(
        &self,
        editor: &ButtonBarEditorState,
        window: &Window,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let base = div()
            .id("button-bar-editor")
            .role(Role::Dialog)
            .aria_label(tr(MSG_LABEL))
            .test_support()
            .track_focus(&editor.focus)
            .key_context(CONTEXT)
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .max_w(px(DIALOG_MAX_W))
            .max_h(px(DIALOG_MAX_H))
            .overflow_hidden()
            .rounded(t.radii.sheet)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.sheet)
            .shadow(t.shadows.menu.clone())
            .text_color(t.colors.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

        self.wire_editor_actions(base, cx)
            .child(self.editor_header(t, cx))
            .when_some(editor.error.as_deref(), |card, message| {
                card.child(editor_error_banner(message, t))
            })
            .child(self.editor_strip(editor, t, cx))
            .child(self.editor_body(editor, window, t, cx))
            .into_any_element()
    }

    fn wire_editor_actions<E: InteractiveElement>(&self, card: E, cx: &mut Context<Self>) -> E {
        card.on_action(
            cx.listener(|view, _: &keys::ButtonBarEditorMoveLeft, window, cx| {
                view.editor_move_selected(-1, window, cx);
            }),
        )
        .on_action(
            cx.listener(|view, _: &keys::ButtonBarEditorMoveRight, window, cx| {
                view.editor_move_selected(1, window, cx);
            }),
        )
        .on_action(
            cx.listener(|view, _: &keys::ButtonBarEditorAdd, window, cx| {
                view.editor_insert_item(default_button_draft(), window, cx);
            }),
        )
        .on_action(
            cx.listener(|view, _: &keys::ButtonBarEditorRemove, window, cx| {
                view.editor_remove_selected(window, cx);
            }),
        )
        .on_action(
            cx.listener(|view, _: &keys::ButtonBarEditorDuplicate, window, cx| {
                view.editor_duplicate_selected(window, cx);
            }),
        )
        .on_action(
            cx.listener(|view, _: &keys::ButtonBarEditorUndo, window, cx| {
                view.editor_undo(window, cx);
            }),
        )
        .on_action(
            cx.listener(|view, _: &keys::ButtonBarEditorDone, window, cx| {
                view.close_button_bar_editor(window, cx);
            }),
        )
    }

    fn editor_header(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let undo = Button::new("button-bar-editor-undo", tr(MSG_UNDO))
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(|view, _, window, cx| view.editor_undo(window, cx)));
        let done = Button::new("button-bar-editor-done", tr(MSG_DONE))
            .variant(ButtonVariant::Primary)
            .on_click(cx.listener(|view, _, window, cx| view.close_button_bar_editor(window, cx)));
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(t.spacing.lg)
            .min_h(px(HEADER_HEIGHT))
            .px(t.spacing.xl)
            .py(t.spacing.md)
            .border_b_1()
            .border_color(t.surfaces.card_edge)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.text)
                    .child(tr(MSG_TITLE)),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(t.spacing.sm)
                    .child(undo)
                    .child(done),
            )
    }

    fn editor_body(
        &self,
        editor: &ButtonBarEditorState,
        window: &Window,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if window.viewport_size().width < px(NARROW_WIDTH) {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(self.editor_narrow_tabs(editor.narrow_pane, t, cx))
                .child(self.editor_pane_content(editor, editor.narrow_pane, t, cx))
                .into_any_element();
        }
        div()
            .id("button-bar-editor-split")
            .test_support()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                self.editor_pane_content(editor, EditorPane::Button, t, cx)
                    .border_r_1()
                    .border_color(t.surfaces.card_edge),
            )
            .child(self.editor_pane_content(editor, EditorPane::Icon, t, cx))
            .into_any_element()
    }

    fn editor_narrow_tabs(
        &self,
        active: EditorPane,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button_tab = editor_tab_button(
            "button-bar-editor-tab-button",
            tr(MSG_TAB_BUTTON),
            active == EditorPane::Button,
            t,
        )
        .on_click(cx.listener(|view, _, _, cx| view.set_editor_pane(EditorPane::Button, cx)));
        let icon_tab = editor_tab_button(
            "button-bar-editor-tab-icon",
            tr(MSG_TAB_ICON),
            active == EditorPane::Icon,
            t,
        )
        .on_click(cx.listener(|view, _, _, cx| view.set_editor_pane(EditorPane::Icon, cx)));
        div()
            .id("button-bar-editor-tabs")
            .role(Role::TabList)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .px(t.spacing.xl)
            .py(t.spacing.sm)
            .border_b_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.field)
            .child(button_tab)
            .child(icon_tab)
            .into_any_element()
    }

    fn editor_pane_content(
        &self,
        editor: &ButtonBarEditorState,
        pane: EditorPane,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> ObservedElement<Stateful<Div>> {
        if pane == EditorPane::Button {
            return self.editor_button_pane(editor, t, cx);
        }
        self.editor_icon_pane(editor, t, cx)
    }

    fn set_editor_pane(&mut self, pane: EditorPane, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.narrow_pane = pane;
            cx.notify();
        }
    }
}

fn editor_error_banner(message: &str, t: &Theme) -> AnyElement {
    div()
        .id("button-bar-editor-error")
        .role(Role::Alert)
        .aria_label(message.to_owned())
        .test_support()
        .flex_none()
        .px(t.spacing.xl)
        .py(t.spacing.sm)
        .border_b_1()
        .border_color(t.surfaces.card_edge)
        .bg(t.surfaces.field)
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(message.to_owned())
        .into_any_element()
}

fn editor_tab_button(
    id: &'static str,
    label: String,
    selected: bool,
    t: &Theme,
) -> ObservedElement<Stateful<Div>> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .role(Role::Tab)
        .aria_label(label.clone())
        .aria_selected(selected)
        .test_support()
        .px(t.spacing.md)
        .py(t.spacing.xs)
        .rounded(t.radii.sm)
        .border_1()
        .cursor_pointer()
        .text_size(t.typography.small)
        .when(selected, |d| {
            d.bg(t.surfaces.card)
                .border_color(t.colors.accent)
                .text_color(t.colors.text)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!selected, |d| {
            d.border_color(t.colors.line.opacity(0.))
                .text_color(t.colors.muted)
                .hover(move |s| s.bg(hover))
        })
        .child(label)
}
