//! Structural edit, undo, and selection handlers for the button bar editor.

use gpui_kit::{AppContext, Context};
use navo_l10n::tr;

use super::{MSG_DEFAULT_BUTTON, MSG_SEPARATOR};
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::{BarEdit, ButtonAction, DraftAction, ItemDraft, ItemPath};

impl NavoWindow {
    pub(super) fn selected_item_label(&self, selected: Option<ItemPath>) -> Option<String> {
        let path = selected?;
        let top = self.button_bar.items.get(path.index)?;
        let item = match path.child {
            None => top,
            Some(child_idx) => match &top.action {
                ButtonAction::Menu(children) => children.get(child_idx)?,
                _ => return None,
            },
        };
        if matches!(item.action, ButtonAction::Separator) {
            Some(tr(MSG_SEPARATOR))
        } else {
            Some(item.name.clone())
        }
    }

    pub(super) fn select_editor_item(&mut self, path: ItemPath, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.selected = Some(path);
            cx.notify();
        }
    }

    pub(super) fn editor_insert_item(&mut self, draft: ItemDraft, cx: &mut Context<Self>) {
        let at = ItemPath {
            index: self.button_bar.items.len(),
            child: None,
        };
        self.run_editor_edit(BarEdit::Insert { at, draft }, Some(Some(at)), cx);
    }

    pub(super) fn editor_move_item(
        &mut self,
        from_index: usize,
        to_index: usize,
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
        self.run_editor_edit(BarEdit::Move { from, to }, Some(Some(to)), cx);
    }

    pub(super) fn editor_move_selected(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(from) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        let Some(to) = self.adjacent_item_path(from, delta) else {
            return;
        };
        self.run_editor_edit(BarEdit::Move { from, to }, Some(Some(to)), cx);
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

    pub(super) fn editor_remove_selected(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        let next = self.selection_after_remove(at);
        self.run_editor_edit(BarEdit::Remove { at }, Some(next), cx);
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

    pub(super) fn editor_duplicate_selected(&mut self, cx: &mut Context<Self>) {
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
        self.run_editor_edit(BarEdit::Duplicate { at }, Some(Some(next)), cx);
    }

    fn run_editor_edit(
        &mut self,
        request: BarEdit,
        next_selected: Option<Option<ItemPath>>,
        cx: &mut Context<Self>,
    ) {
        let Some(edit_fn) = self.button_bar.hooks.as_ref().map(|h| h.edit.clone()) else {
            return;
        };
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.error = None;
        }
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { edit_fn(request) }).await;
            let _ = this.update(cx, |view, cx| {
                view.finish_editor_hook(result, next_selected, cx);
            });
        })
        .detach();
    }

    pub(super) fn editor_undo(&mut self, cx: &mut Context<Self>) {
        let Some(undo_fn) = self.button_bar.hooks.as_ref().map(|h| h.undo.clone()) else {
            return;
        };
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.error = None;
        }
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { undo_fn() }).await;
            let _ = this.update(cx, |view, cx| {
                view.finish_editor_hook(result, None, cx);
            });
        })
        .detach();
    }

    fn finish_editor_hook(
        &mut self,
        result: Result<(), String>,
        next_selected: Option<Option<ItemPath>>,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        match result {
            Ok(()) => {
                editor.error = None;
                if let Some(selected) = next_selected {
                    editor.selected = selected;
                }
            }
            Err(message) => {
                editor.error = Some(message);
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
