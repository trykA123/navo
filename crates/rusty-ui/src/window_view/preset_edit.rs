use std::sync::Arc;

use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt};
use rusty_core::Tiles;
use rusty_l10n::FluentArgs;

use super::{FerriteWindow, SaveReport};
use crate::confirm_dialog::{ConfirmDialog, ConfirmProps};
use crate::prelude::*;
use crate::prompt_dialog::{PromptDialog, PromptProps};
use crate::status_bar::StatusNotice;
use crate::tokens;

pub const MAX_LAYOUT_PRESETS: usize = 20;
const MAX_NAME_CHARS: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutEdit {
    Save { name: String, tiles: String },
    Replace { name: String, tiles: String },
    Remove { name: String },
}

pub type EditLayout = Arc<dyn Fn(&LayoutEdit) -> Result<SaveReport, String> + Send + Sync>;

pub fn no_layout_edits() -> EditLayout {
    Arc::new(|_| Err(rusty_l10n::tr("layout-presets-unavailable")))
}

enum Dialog {
    Name { error: Option<SharedString> },
    Ask(LayoutEdit),
}

#[derive(Default)]
pub(super) struct PresetDialog {
    id: u64,
    dialog: Option<Dialog>,
}

#[cfg(test)]
impl PresetDialog {
    pub(super) fn is_naming(&self) -> bool {
        matches!(self.dialog, Some(Dialog::Name { .. }))
    }

    pub(super) fn name_error(&self) -> Option<String> {
        match &self.dialog {
            Some(Dialog::Name { error }) => error.as_ref().map(ToString::to_string),
            _ => None,
        }
    }

    pub(super) fn asking(&self) -> Option<&LayoutEdit> {
        match &self.dialog {
            Some(Dialog::Ask(edit)) => Some(edit),
            _ => None,
        }
    }
}

fn with_name(id: &str, name: &str) -> SharedString {
    let mut args = FluentArgs::new();
    args.set("name", name.to_owned());
    rusty_l10n::tr_args(id, &args).into()
}

fn free_name(taken: &[(SharedString, Tiles)]) -> String {
    (1..)
        .map(|n| {
            let mut args = FluentArgs::new();
            args.set("n", n);
            rusty_l10n::tr_args("layout-save-default", &args)
        })
        .find(|name| taken.iter().all(|(used, _)| used.as_ref() != name))
        .unwrap_or_default()
}

pub fn layout_ask_props(edit: &LayoutEdit) -> Option<ConfirmProps> {
    let (name, [title, summary, confirm], danger) = match edit {
        LayoutEdit::Replace { name, .. } => (
            name,
            [
                "layout-replace-title",
                "layout-replace-summary",
                "layout-replace-confirm",
            ],
            false,
        ),
        LayoutEdit::Remove { name } => (
            name,
            [
                "layout-remove-title",
                "layout-remove-summary",
                "layout-remove-confirm",
            ],
            true,
        ),
        LayoutEdit::Save { .. } => return None,
    };
    Some(ConfirmProps {
        title: with_name(title, name),
        lines: vec![name.clone().into()],
        more: 0,
        summary: rusty_l10n::tr(summary).into(),
        confirm_label: rusty_l10n::tr(confirm).into(),
        danger,
        typed: None,
        mode: None,
        choice: None,
    })
}

pub fn save_layout_props(
    user: &[(SharedString, Tiles)],
    error: Option<SharedString>,
) -> PromptProps {
    let value = free_name(user);
    PromptProps {
        title: rusty_l10n::tr("layout-save-title").into(),
        label: rusty_l10n::tr("layout-save-label").into(),
        select: 0..value.len(),
        value: value.into(),
        confirm_label: rusty_l10n::tr("layout-save-confirm").into(),
        error,
    }
}

fn scrim(dialog: impl IntoElement, reduce_motion: bool, cx: &App) -> AnyElement {
    let layer = div()
        .id("layout-preset-scrim")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui_kit::black().opacity(0.4))
        .occlude()
        .child(dialog);
    if reduce_motion {
        return layer.into_any_element();
    }
    let fade = Animation::new(tokens::current(cx).motion.fade).with_easing(ease_out_cubic);
    layer
        .with_animation("layout-preset-enter", fade, |layer, delta| {
            layer.opacity(delta)
        })
        .into_any_element()
}

impl FerriteWindow {
    fn open_preset_dialog(&mut self, dialog: Dialog, cx: &mut Context<Self>) {
        self.browser.dialog_opened();
        self.tiles.presets.id += 1;
        self.tiles.presets.dialog = Some(dialog);
        cx.notify();
    }

    pub(super) fn ask_save_layout(&mut self, cx: &mut Context<Self>) {
        self.open_preset_dialog(Dialog::Name { error: None }, cx);
    }

    pub(super) fn ask_remove_layout(&mut self, focused: usize, cx: &mut Context<Self>) {
        let user = focused.checked_sub(Tiles::builtin().len());
        let Some((name, _)) = user.and_then(|at| self.values.layout_presets().get(at)) else {
            return;
        };
        let name = name.to_string();
        self.open_preset_dialog(Dialog::Ask(LayoutEdit::Remove { name }), cx);
    }

    fn submit_layout_name(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = value.trim();
        if !(1..=MAX_NAME_CHARS).contains(&name.chars().count()) {
            let error = Some(rusty_l10n::tr("layout-save-name-length").into());
            self.tiles.presets.dialog = Some(Dialog::Name { error });
            cx.notify();
            return;
        }
        let tiles = self.tiles_layout().to_tiles().print();
        let name = name.to_owned();
        let exists = self.values.layout_presets().iter().any(|(n, _)| *n == name);
        if exists {
            let edit = LayoutEdit::Replace { name, tiles };
            self.open_preset_dialog(Dialog::Ask(edit), cx);
        } else if self.values.layout_presets().len() >= MAX_LAYOUT_PRESETS {
            let error = Some(rusty_l10n::tr("layout-presets-full").into());
            self.tiles.presets.dialog = Some(Dialog::Name { error });
            cx.notify();
        } else {
            self.run_layout_edit(LayoutEdit::Save { name, tiles }, window, cx);
        }
    }

    fn answer_layout_ask(&mut self, confirmed: bool, window: &mut Window, cx: &mut Context<Self>) {
        match self.tiles.presets.dialog.take() {
            Some(Dialog::Ask(edit)) if confirmed => self.run_layout_edit(edit, window, cx),
            _ => self.close_preset_dialog(window, cx),
        }
    }

    fn close_preset_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tiles.presets.dialog = None;
        match &self.tiles.switcher {
            Some(state) => window.focus(&state.focus, cx),
            None => self.focus_core_pane(window, cx),
        }
        cx.notify();
    }

    fn run_layout_edit(&mut self, edit: LayoutEdit, window: &mut Window, cx: &mut Context<Self>) {
        self.close_preset_dialog(window, cx);
        let hook = self.saver.edit_layout.clone();
        let task = cx.background_executor().spawn(async move { hook(&edit) });
        cx.spawn_in(window, async move |this, cx| {
            let outcome = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.layout_edited(outcome, window, cx);
            });
        })
        .detach();
    }

    fn layout_edited(
        &mut self,
        outcome: Result<SaveReport, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let problem = match outcome {
            Ok(report) => {
                let fresh = self.saver.on_disk(report.values, report.generation);
                self.set_values(fresh, window, cx);
                report.error
            }
            Err(error) => Some(error),
        };
        if let Some(detail) = problem {
            let mut args = FluentArgs::new();
            args.set("detail", detail);
            self.show_notice(StatusNotice {
                text: rusty_l10n::tr_args("layout-presets-failed", &args).into(),
                is_error: true,
            });
        }
        cx.notify();
    }

    fn name_dialog(&self, error: Option<SharedString>, cx: &mut Context<Self>) -> AnyElement {
        let props = save_layout_props(self.values.layout_presets(), error);
        let (submit, cancel) = (cx.weak_entity(), cx.weak_entity());
        PromptDialog::new(("layout-save", self.tiles.presets.id), props)
            .on_submit(move |value, window, cx| {
                let value = value.to_string();
                let _ = submit.update(cx, |view, cx| view.submit_layout_name(&value, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.close_preset_dialog(window, cx));
            })
            .into_any_element()
    }

    fn ask_dialog(&self, props: ConfirmProps, cx: &mut Context<Self>) -> AnyElement {
        let (confirm, cancel) = (cx.weak_entity(), cx.weak_entity());
        ConfirmDialog::new(("layout-ask", self.tiles.presets.id), props)
            .on_confirm(move |window, cx| {
                let _ = confirm.update(cx, |view, cx| view.answer_layout_ask(true, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.answer_layout_ask(false, window, cx));
            })
            .into_any_element()
    }

    pub(super) fn preset_dialog_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = match self.tiles.presets.dialog.as_ref()? {
            Dialog::Name { error } => self.name_dialog(error.clone(), cx),
            Dialog::Ask(edit) => self.ask_dialog(layout_ask_props(edit)?, cx),
        };
        Some(scrim(dialog, cx.reduce_motion(), cx))
    }
}
