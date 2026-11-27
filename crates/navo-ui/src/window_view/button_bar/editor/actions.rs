//! Structural edit, undo, and selection handlers for the button bar editor.

use gpui_kit::{AppContext, Context, Window};
use navo_l10n::tr;

use super::MSG_DEFAULT_BUTTON;
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::{
    BarEdit, ButtonAction, ButtonItem, DraftAction, ItemDraft, ItemPath,
};

impl NavoWindow {
    pub(super) fn selected_item(&self, selected: Option<ItemPath>) -> Option<&ButtonItem> {
        let path = selected?;
        let top = self.button_bar.items.get(path.index)?;
        match path.child {
            None => Some(top),
            Some(child_idx) => match &top.action {
                ButtonAction::Menu(children) => children.get(child_idx),
                _ => None,
            },
        }
    }

    pub(super) fn sync_form_from_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            if let Some(editor) = self.button_bar.editor.as_mut() {
                editor.form.sync_from_item(None, None, window, cx);
                editor.picker.maker.sync_from_spec(None, window, cx);
            }
            return;
        };
        let item = self.selected_item(Some(selected)).cloned();
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor
                .form
                .sync_from_item(Some(selected), item.as_ref(), window, cx);
            editor.picker.maker.sync_from_spec(
                item.as_ref().and_then(|entry| entry.icon_spec.as_ref()),
                window,
                cx,
            );
        }
    }

    pub(super) fn select_editor_item(
        &mut self,
        path: ItemPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.selected = Some(path);
            self.sync_form_from_selected(window, cx);
            cx.notify();
        }
    }

    pub(super) fn editor_insert_item(
        &mut self,
        draft: ItemDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = ItemPath {
            index: self.button_bar.items.len(),
            child: None,
        };
        self.run_editor_edit(BarEdit::Insert { at, draft }, Some(Some(at)), window, cx);
    }

    pub(super) fn editor_move_item(
        &mut self,
        from_index: usize,
        to_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let len = self.button_bar.items.len();
        if from_index == to_index || from_index >= len || to_index >= len {
            return;
        }
        let from = ItemPath {
            index: from_index,
            child: None,
        };
        let to = ItemPath {
            index: to_index,
            child: None,
        };
        self.run_editor_edit(BarEdit::Move { from, to }, Some(Some(to)), window, cx);
    }

    pub(super) fn editor_move_selected(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        let Some(to) = self.adjacent_item_path(from, delta) else {
            return;
        };
        self.run_editor_edit(BarEdit::Move { from, to }, Some(Some(to)), window, cx);
    }

    fn adjacent_item_path(&self, from: ItemPath, delta: isize) -> Option<ItemPath> {
        match from.child {
            None => {
                let target = from.index.checked_add_signed(delta)?;
                (target < self.button_bar.items.len() && target != from.index).then_some(ItemPath {
                    index: target,
                    child: None,
                })
            }
            Some(child_idx) => {
                let ButtonAction::Menu(children) = &self.button_bar.items.get(from.index)?.action
                else {
                    return None;
                };
                let target = child_idx.checked_add_signed(delta)?;
                (target < children.len() && target != child_idx).then_some(ItemPath {
                    index: from.index,
                    child: Some(target),
                })
            }
        }
    }

    pub(super) fn editor_remove_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        let next = self.selection_after_remove(at);
        self.run_editor_edit(BarEdit::Remove { at }, Some(next), window, cx);
    }

    fn selection_after_remove(&self, at: ItemPath) -> Option<ItemPath> {
        match at.child {
            None => {
                let len = self.button_bar.items.len();
                (len > 1).then(|| ItemPath {
                    index: at.index.min(len - 2),
                    child: None,
                })
            }
            Some(child_idx) => {
                let count = match &self.button_bar.items.get(at.index)?.action {
                    ButtonAction::Menu(children) => children.len(),
                    _ => 0,
                };
                if count > 1 {
                    Some(ItemPath {
                        index: at.index,
                        child: Some(child_idx.min(count - 2)),
                    })
                } else {
                    Some(ItemPath {
                        index: at.index,
                        child: None,
                    })
                }
            }
        }
    }

    pub(super) fn editor_duplicate_selected(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        let next = match at.child {
            None => ItemPath {
                index: at.index + 1,
                child: None,
            },
            Some(c) => ItemPath {
                index: at.index,
                child: Some(c + 1),
            },
        };
        self.run_editor_edit(BarEdit::Duplicate { at }, Some(Some(next)), window, cx);
    }

    fn run_editor_edit(
        &mut self,
        request: BarEdit,
        next_selected: Option<Option<ItemPath>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit_fn) = self.button_bar.hooks.as_ref().map(|h| h.edit.clone()) else {
            return;
        };
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.error = None;
        }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { edit_fn(request) }).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.finish_editor_hook(result, next_selected, window, cx);
            });
        })
        .detach();
    }

    pub(super) fn editor_undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(undo_fn) = self.button_bar.hooks.as_ref().map(|h| h.undo.clone()) else {
            return;
        };
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.error = None;
        }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { undo_fn() }).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.finish_editor_hook(result, None, window, cx);
            });
        })
        .detach();
    }

    fn finish_editor_hook(
        &mut self,
        result: Result<(), String>,
        next_selected: Option<Option<ItemPath>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        let resync = match result {
            Ok(()) => {
                editor.error = None;
                editor.selected = next_selected.unwrap_or(editor.selected);
                editor.form.force_resync = true;
                true
            }
            Err(message) => {
                editor.error = Some(message);
                false
            }
        };
        if resync {
            self.sync_form_from_selected(window, cx);
            if let Some(editor) = self.button_bar.editor.as_mut() {
                editor.form.force_resync = true;
            }
        }
        cx.notify();
    }
}

pub(super) fn default_button_draft() -> ItemDraft {
    ItemDraft {
        name: tr(MSG_DEFAULT_BUTTON),
        action: DraftAction::Navo(String::from("files.refresh")),
        icon: None,
    }
}
