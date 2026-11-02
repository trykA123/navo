//! What the selected row can do in the window: the Ctrl+K menu, the keys that run its
//! actions, and the status line that says what happened.

use gpui_kit::component::input;
use gpui_kit::{Context, Window};
use navo_core::Location;
use navo_l10n::tr;

use super::chrome::enter_label;
use super::preview::full_text;
use super::secret::{Facts, Revealed};
use super::status::Status;
use super::view::{ClipPalette, ClipPaletteEvent, Place};
use super::{ClipKind, ClipRow, How, Origin};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    /// Enter: copy, or go to a folder path; from anywhere, paste.
    Enter,
    CopyOnly,
    PasteFiles,
    /// Alt+R: show the secret in the preview, or hide it again.
    Reveal,
    /// Alt+U: copy the user name of a secret.
    CopyUser,
    /// Ctrl+S: save a history row as a snippet or a path item.
    Save,
    Delete,
    /// F2 on a history row: the library view.
    Manage,
}

/// One line of the actions menu.
pub(super) struct Item {
    pub action: Action,
    pub label: String,
    pub key: &'static str,
}

fn item(action: Action, label_key: &str, key: &'static str) -> Item {
    Item {
        action,
        label: tr(label_key),
        key,
    }
}

fn save_item(row: &ClipRow) -> Option<Item> {
    let label = match row.kind {
        ClipKind::Text | ClipKind::Link => "clip-act-save-snippet",
        ClipKind::Path | ClipKind::Files => "clip-act-save-path",
        ClipKind::Image | ClipKind::Secret => return None,
    };
    (row.from == Origin::Clipboard).then(|| item(Action::Save, label, "ctrl-s"))
}

/// The menu for a row, in the prototype's order: Enter, the extra copy or paste, the secret
/// and save items, Delete, Manage. F2 edits a saved row, so Manage shows no key there.
pub(super) fn items(row: &ClipRow, facts: Facts) -> Vec<Item> {
    let enter = Item {
        action: Action::Enter,
        label: enter_label(row, facts.place),
        key: "enter",
    };
    let extra = match (facts.place, row.kind) {
        (Place::Window, ClipKind::Path) => {
            Some(item(Action::CopyOnly, "clip-act-copy-path", "ctrl-c"))
        }
        (Place::Window, ClipKind::Files) => {
            Some(item(Action::PasteFiles, "clip-act-paste-here", "ctrl-v"))
        }
        (Place::Anywhere, kind) if kind != ClipKind::Secret => {
            Some(item(Action::CopyOnly, "clip-act-copy-only", "ctrl-c"))
        }
        _ => None,
    };
    let is_secret = row.kind == ClipKind::Secret;
    let reveal = is_secret.then(|| {
        let label = if facts.is_shown {
            "clip-act-hide"
        } else {
            "clip-act-show"
        };
        item(Action::Reveal, label, "alt-r")
    });
    let user = (is_secret && facts.has_user)
        .then(|| item(Action::CopyUser, "clip-act-copy-user", "alt-u"));
    let delete = item(Action::Delete, "clip-act-delete", "delete");
    let manage_key = if row.from == Origin::Saved { "" } else { "f2" };
    let manage = item(Action::Manage, "clip-act-manage", manage_key);
    std::iter::once(enter)
        .chain(extra)
        .chain(reveal)
        .chain(user)
        .chain(save_item(row))
        .chain([delete, manage])
        .collect()
}

/// The open menu (the selected item), the latest status line and the secret on show.
#[derive(Default)]
pub(super) struct Overlay {
    pub menu: Option<usize>,
    pub status: Option<Status>,
    pub revealed: Option<Revealed>,
    pub serial: u64,
}

enum Done {
    Copied,
    CopiedHidden,
    CopiedUser,
    Deleted,
    Saved(ClipKind),
    Toggled,
    WentTo(Location),
    Pasting,
    PastingBack,
    Managing,
}

fn folder_of(row: &ClipRow) -> Result<Location, String> {
    let line = full_text(row).lines().next().unwrap_or_default().trim();
    Location::new(line).map_err(|_| tr("clip-status-bad-path"))
}

impl ClipPalette {
    fn perform(&mut self, action: Action, row: &ClipRow) -> Result<Done, String> {
        let anywhere = self.place == Place::Anywhere;
        match (action, row.kind) {
            (Action::Enter, _) if anywhere => self
                .hooks
                .copy(row.id, How::Paste)
                .map(|()| Done::PastingBack),
            (Action::Enter, ClipKind::Path) => folder_of(row).map(Done::WentTo),
            (Action::Enter | Action::CopyOnly, ClipKind::Secret) => self
                .hooks
                .copy(row.id, How::Copy)
                .map(|()| Done::CopiedHidden),
            (Action::Enter | Action::CopyOnly, _) => {
                self.hooks.copy(row.id, How::Copy).map(|()| Done::Copied)
            }
            (Action::PasteFiles, _) => self.hooks.copy(row.id, How::Copy).map(|()| Done::Pasting),
            (Action::Reveal, _) => self.flip_reveal(row.id).map(|()| Done::Toggled),
            (Action::CopyUser, _) => self.hooks.copy_user(row.id).map(|()| Done::CopiedUser),
            (Action::Save, kind) => self
                .hooks
                .save_from_history(row.id)
                .map(|_| Done::Saved(kind)),
            (Action::Delete, _) => self.hooks.delete(row.id).map(|()| Done::Deleted),
            (Action::Manage, _) => Ok(Done::Managing),
        }
    }

    /// Runs an action on the selected row and closes the menu. The palette stays open.
    pub(super) fn run(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.manage.is_some() {
            return;
        }
        self.overlay.menu = None;
        let Some(row) = self.state.selected_row().cloned() else {
            cx.notify();
            return;
        };
        match self.perform(action, &row) {
            Ok(done) => self.finish(done, cx),
            Err(message) => self.say(message, true, cx),
        }
    }

    fn finish(&mut self, done: Done, cx: &mut Context<Self>) {
        let (key, event) = match done {
            Done::Copied => ("clip-status-copied", None),
            Done::Deleted => ("clip-status-deleted", None),
            Done::Saved(ClipKind::Path | ClipKind::Files) => ("clip-status-saved-path", None),
            Done::Saved(_) => ("clip-status-saved-snippet", None),
            Done::WentTo(location) => ("clip-status-went", Some(ClipPaletteEvent::GoTo(location))),
            Done::Pasting => ("clip-status-pasting", Some(ClipPaletteEvent::PasteIntoPane)),
            Done::PastingBack => return cx.emit(ClipPaletteEvent::PasteBack),
            Done::CopiedHidden => return self.say_clearing(tr("clip-status-copied-hidden"), cx),
            Done::CopiedUser => return self.say_clearing(tr("clip-status-copied-user"), cx),
            Done::Toggled => return cx.notify(),
            Done::Managing => return self.open_manage(cx),
        };
        self.say(tr(key), false, cx);
        if let Some(event) = event {
            cx.emit(event);
        }
    }

    /// Enter on the selected row.
    pub fn copy_selected(&mut self, cx: &mut Context<Self>) {
        self.run(Action::Enter, cx);
    }

    /// Opens the menu for the selected row; nothing when there is no row or it is open.
    pub fn open_actions(&mut self, cx: &mut Context<Self>) {
        if self.overlay.menu.is_none() {
            self.toggle_actions(cx);
        }
    }

    /// Ctrl+K and the Actions button: opens the menu for the selected row, or closes it.
    pub(super) fn toggle_actions(&mut self, cx: &mut Context<Self>) {
        if self.manage.is_some() {
            return;
        }
        if self.overlay.menu.take().is_none() && self.state.selected_row().is_some() {
            self.overlay.menu = Some(0);
        }
        cx.notify();
    }

    /// False when no menu was open.
    pub(super) fn close_actions(&mut self, cx: &mut Context<Self>) -> bool {
        let was_open = self.overlay.menu.take().is_some();
        if was_open {
            cx.notify();
        }
        was_open
    }

    /// Moves the menu selection, wrapping; false when no menu is open.
    pub(super) fn step_actions(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let (Some(at), Some(row)) = (self.overlay.menu, self.state.selected_row()) else {
            return false;
        };
        let count = items(row, self.facts(row)).len();
        self.overlay.menu = Some(if delta.is_negative() {
            at.checked_sub(1).unwrap_or(count - 1)
        } else {
            (at + 1) % count
        });
        cx.notify();
        true
    }

    pub(super) fn point_at_action(&mut self, at: usize, cx: &mut Context<Self>) {
        if self.overlay.menu.is_some_and(|open| open != at) {
            self.overlay.menu = Some(at);
            cx.notify();
        }
    }

    /// Enter: the highlighted menu item when the menu is open, else the row's own action.
    pub(super) fn choose(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.state.selected_row() else {
            return;
        };
        let action = self
            .overlay
            .menu
            .and_then(|at| items(row, self.facts(row)).get(at).map(|item| item.action))
            .unwrap_or(Action::Enter);
        self.run(action, cx);
    }

    /// Ctrl+C copies the row, unless the search field has selected text to copy.
    pub(super) fn copy_only(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.field.read(cx).selected_value().is_empty() {
            self.run(Action::CopyOnly, cx);
        } else {
            window.dispatch_action(Box::new(input::Copy), cx);
        }
    }

    /// Ctrl+V pastes a file list into the active pane; elsewhere, and from anywhere, it pastes
    /// into the field.
    pub(super) fn paste_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_files = self
            .state
            .selected_row()
            .is_some_and(|row| row.kind == ClipKind::Files);
        if has_files && self.place == Place::Window && self.manage.is_none() {
            self.run(Action::PasteFiles, cx);
        } else {
            window.dispatch_action(Box::new(input::Paste), cx);
        }
    }

    /// Del deletes the row, unless a search is typed: then it edits the text.
    pub(super) fn delete_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.query(cx).is_empty() {
            window.dispatch_action(Box::new(input::Delete), cx);
        } else if self.manage.is_some() {
            self.manage_delete(cx);
        } else {
            self.run(Action::Delete, cx);
        }
    }
}
