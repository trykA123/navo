use gpui_kit::{Focusable, KeyContext, Stateful, Window};

use super::{FerriteWindow, inputs};
use crate::dialog_focus::{self, Next, Previous};
use crate::file_list::MultiRename;
use crate::multi_rename::view::fields::{Fields, Mask};
use crate::multi_rename::{RenamePanel, keys as mr};
use crate::prelude::*;
use crate::tokens;

pub(super) const PREVIEW_CONTEXT: &str = "MultiRenamePreview";

pub(super) fn contexts(names: &[&'static str]) -> KeyContext {
    let mut context = KeyContext::default();
    for name in names {
        context.add(*name);
    }
    context
}

impl FerriteWindow {
    pub(in crate::window_view) fn modal_view(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.dialog(cx)
            .or_else(|| self.prompt_view(cx))
            .or_else(|| self.recent_ask_view(cx))
            .or_else(|| self.multi_rename_view(window, cx))
    }

    pub(in crate::window_view) fn multi_rename_on(root: Div, cx: &mut Context<Self>) -> Div {
        Self::undo_on(root, cx).on_action(cx.listener(|view, _: &MultiRename, window, cx| {
            view.open_multi_rename(window, cx);
        }))
    }

    fn rename_preview_area(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let open = self.multi_rename.open.as_ref()?;
        let t = tokens::current(cx).clone();
        let ring = if open.preview_focus.is_focused(window) {
            t.colors.accent
        } else {
            gpui_kit::transparent_black()
        };
        Some(
            div()
                .id("rename-preview-area")
                .key_context(PREVIEW_CONTEXT)
                .track_focus(&open.preview_focus)
                .flex()
                .flex_1()
                .min_w_0()
                .border_2()
                .border_color(ring)
                .on_action(cx.listener(|view, _: &mr::ToggleRow, window, cx| {
                    view.toggle_cursor_row(window, cx);
                }))
                .on_action(cx.listener(|view, _: &mr::MoveRowUp, window, cx| {
                    view.move_rename_row(false, window, cx)
                }))
                .on_action(cx.listener(|view, _: &mr::MoveRowDown, window, cx| {
                    view.move_rename_row(true, window, cx)
                }))
                .flex_col()
                .children(self.rename_names_editor(cx))
                .child(open.view.clone()),
        )
    }

    fn multi_rename_view(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.multi_rename.open.as_ref()?;
        let weak = cx.weak_entity();
        let fields = Fields::new("rename-fields", &open.inputs, &open.form.rules)
            .errors(open.errors.clone())
            .on_event(move |event, window, cx| {
                let _ = weak.update(cx, |view, cx| view.rename_fields_event(event, window, cx));
            });
        let fields = self.with_field_menu(fields, cx);
        let preview = self.rename_preview_area(window, cx)?;
        let panel = RenamePanel::new(open.title.clone(), fields, preview)
            .tools(self.rename_tools(cx))
            .problems_only(open.form.problems_only)
            .on_problems_only({
                let weak = cx.weak_entity();
                move |_, cx| {
                    let _ = weak.update(cx, |view, cx| view.toggle_problems_only(cx));
                }
            })
            .on_presets({
                let weak = cx.weak_entity();
                move |window, cx| {
                    let _ = weak.update(cx, |view, cx| view.ask_presets(window, cx));
                }
            });
        let panel = match self.preset_menu_view(cx) {
            Some(menu) => panel.presets_menu(menu),
            None => panel,
        };
        let root = crate::multi_rename::scrim()
            .id("multi-rename-scrim")
            .occlude()
            .key_context(contexts(&["MultiRename", "Dialog"]))
            .track_focus(&open.root)
            .child(panel);
        let root = Self::rename_actions(root, cx);
        Some(Self::rename_name_actions(root, cx).into_any_element())
    }

    fn rename_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &mr::Rename, window, cx| {
            view.run_multi_rename(window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::Close, window, cx| {
            view.escape_multi_rename(window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::TokenMenu, window, cx| {
            view.token_menu_key(window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::FocusPreview, window, cx| {
            view.focus_rename_preview(window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::FocusFields, window, cx| {
            view.focus_rename_fields(window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::PageUp, _, cx| view.page_rename(false, cx)))
        .on_action(cx.listener(|view, _: &mr::PageDown, _, cx| view.page_rename(true, cx)))
        .on_action(cx.listener(|view, _: &mr::NextProblem, window, cx| {
            view.step_problem(true, window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::PreviousProblem, window, cx| {
            view.step_problem(false, window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::SavePreset, window, cx| {
            view.ask_save_preset(window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::OpenPresets, window, cx| {
            view.ask_presets(window, cx);
        }))
        .on_action(cx.listener(|view, _: &mr::Reset, window, cx| {
            view.reset_multi_rename(window, cx);
        }))
        .on_action(cx.listener(|view, _: &Next, window, cx| view.cycle_rename(true, window, cx)))
        .on_action(cx.listener(|view, _: &Previous, window, cx| {
            view.cycle_rename(false, window, cx);
        }))
    }

    fn escape_multi_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_rename_menu(window, cx) {
            return;
        }
        self.close_multi_rename(window, cx);
    }

    fn token_menu_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let in_extension = open
            .inputs
            .ext_mask
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let mask = if in_extension {
            Mask::Extension
        } else {
            Mask::Name
        };
        self.open_rename_menu(super::menu::Kind::Tokens(mask), window, cx);
    }

    pub(super) fn remember_rename_field(&mut self, window: &Window, cx: &App) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        if let Some(field) = inputs::stops(&open.inputs, cx)
            .into_iter()
            .find(|stop| stop.is_focused(window))
        {
            open.last_field = field;
        }
    }

    pub(super) fn focus_rename_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_rename_field(window, cx);
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        open.view.update(cx, |view, cx| {
            if view.cursor().is_none() {
                let first = view.first_shown();
                view.set_cursor(first, cx);
            }
        });
        window.focus(&open.preview_focus, cx);
        cx.notify();
    }

    pub(super) fn focus_rename_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.multi_rename.open.as_ref() {
            window.focus(&open.last_field, cx);
            cx.notify();
        }
    }

    fn page_rename(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(open) = self.multi_rename.open.as_ref() {
            open.view.update(cx, |view, cx| view.page(forward, cx));
        }
    }

    fn toggle_cursor_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row = self
            .multi_rename
            .open
            .as_ref()
            .and_then(|open| open.view.read(cx).cursor());
        if let Some(row) = row {
            self.toggle_rename_row(row, window, cx);
        }
    }

    fn cycle_rename(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let mut stops = inputs::stops(&open.inputs, cx);
        if let Some(editor) = &open.names {
            stops.push(editor.input.read(cx).focus_handle(cx));
        }
        stops.push(open.preview_focus.clone());
        dialog_focus::cycle(&stops, forward, window, cx);
        cx.notify();
    }

    fn reset_multi_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        open.form.reset();
        open.names = None;
        inputs::fill(&open.inputs, &open.form.rules, window, cx);
        self.rules_changed(window, cx);
    }
}
