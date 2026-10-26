use std::time::Duration;

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{ClipboardItem, FocusHandle, Focusable, PathPromptOptions, Subscription};
use rusty_l10n::{t, tr};

use super::{FerriteWindow, compute, names_file};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

pub(super) struct Editor {
    pub input: Entity<TextareaState>,
    _changed: Subscription,
}

struct Imported {
    root: FocusHandle,
    generation: u64,
    result: Result<String, String>,
}

fn editor(text: String, window: &mut Window, cx: &mut Context<FerriteWindow>) -> Editor {
    let input = cx.new(|cx| TextareaState::new(window, cx).default_value(text));
    let changed = cx.subscribe_in(&input, window, |view, input, event, window, cx| {
        if !matches!(event, InputEvent::Change) {
            return;
        }
        let text = input.read(cx).value().to_string();
        view.rename_names_changed(&text, window, cx);
    });
    Editor {
        input,
        _changed: changed,
    }
}

impl FerriteWindow {
    pub(super) fn edit_rename_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_rename_names(window, cx);
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        if open.names.take().is_some() {
            return self.focus_rename_preview(window, cx);
        }
        if open.form.edited_names().is_none() && !open.is_current() {
            return;
        }
        let text = open
            .form
            .edited_names()
            .map(|names| names.join("\n"))
            .or_else(|| {
                open.preview.as_ref().map(|preview| {
                    preview
                        .rows
                        .iter()
                        .map(|row| row.new.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            });
        if let Some(text) = text {
            self.set_rename_names(text, window, cx);
        }
    }

    pub(super) fn set_rename_names(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        open.form.set_edited_names(&text);
        if open.names.is_none() {
            open.names = Some(editor(text, window, cx));
        } else if let Some(editor) = &open.names {
            editor
                .input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
        if let Some(editor) = &open.names {
            window.focus(&editor.input.read(cx).focus_handle(cx), cx);
        }
        self.schedule_rename_preview(Duration::ZERO, window, cx);
        cx.notify();
    }

    fn rename_names_changed(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.multi_rename.open.as_mut() {
            let before = open.form.clone();
            open.form.set_edited_names(text);
            if before == open.form {
                return;
            }
        }
        self.schedule_rename_preview(compute::DEBOUNCE, window, cx);
        cx.notify();
    }

    pub(super) fn sync_rename_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .multi_rename
            .open
            .as_ref()
            .and_then(|open| open.names.as_ref())
            .map(|editor| editor.input.read(cx).value().to_string());
        if let Some(text) = text {
            self.rename_names_changed(&text, window, cx);
        }
    }

    pub(super) fn paste_rename_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match cx.read_from_clipboard().and_then(|item| item.text()) {
            Some(text) => self.set_rename_names(text, window, cx),
            None => self.notify_rename("rename-names-clipboard-empty", cx),
        }
    }

    pub(super) fn copy_rename_names(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self
            .multi_rename
            .open
            .as_ref()
            .filter(|open| open.is_current())
        else {
            return;
        };
        let Some(preview) = &open.preview else { return };
        let separator = if cfg!(windows) { "\r\n" } else { "\n" };
        let text = preview
            .rows
            .iter()
            .map(|row| row.new.as_str())
            .collect::<Vec<_>>()
            .join(separator);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(super) fn load_rename_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.multi_rename.open else {
            return;
        };
        let (root, generation) = (open.root.clone(), open.generation);
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr("rename-load-names").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let loaded = match paths.await {
                Ok(Ok(Some(paths))) => cx.background_spawn(names_file::read_paths(paths)).await,
                Ok(Ok(None)) => return,
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.apply_rename_import(
                    Imported {
                        root,
                        generation,
                        result: loaded,
                    },
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    fn apply_rename_import(
        &mut self,
        import: Imported,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .multi_rename
            .open
            .as_ref()
            .is_none_or(|open| open.root != import.root || open.generation != import.generation)
        {
            return;
        }
        match import.result {
            Ok(text) => self.set_rename_names(text, window, cx),
            Err(reason) => {
                self.show_notice(StatusNotice {
                    text: t!("rename-names-load-failed", reason = reason).into(),
                    is_error: true,
                });
                cx.notify();
            }
        }
    }

    pub(super) fn toggle_folder_counter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.multi_rename.open.as_mut() {
            open.form.counter_per_folder = !open.form.counter_per_folder;
        }
        self.schedule_rename_preview(compute::DEBOUNCE, window, cx);
        cx.notify();
    }
}
