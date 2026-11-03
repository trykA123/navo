//! The note editor of Notes mode: one editor for the selected note. A change is saved 500 ms
//! after typing stops, and any edit still waiting is saved when the selection moves, the
//! palette hides or the palette closes. Note text stays out of logs and `Debug`.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{self, EditorState, InputEvent};
use gpui_kit::{Action, App, AppContext as _, Context, Entity, Focusable, Subscription, Window};
use navo_core::Location;
use navo_l10n::t;

use super::notes::is_placeholder;
use super::view::ClipPalette;
use super::{ClipKind, ClipMode, Scope};

const SAVE_AFTER: Duration = Duration::from_millis(500);
const PROBE_LIMIT: usize = 200;

/// Tells whether a folder is known to be gone; false when unsure, as for a share that is
/// offline. Runs off the UI thread.
pub type FolderProbe = Arc<dyn Fn(&Location) -> bool + Send + Sync>;

/// An editor for plain text: no line numbers, folding or search.
pub(super) fn plain_editor(window: &mut Window, cx: &mut Context<EditorState>) -> EditorState {
    let mut state = EditorState::new(window, cx).line_number(false);
    state.set_folding(false, window, cx);
    state.set_searchable(false, cx);
    state
}

/// The note the editor holds, and the text the hooks last had for it.
struct Held {
    scope: Scope,
    baseline: String,
    /// False when the hooks could not give the text of a note that exists: the editor stays
    /// read-only, so that nothing replaces what it could not show.
    is_readable: bool,
}

pub(super) struct NoteEditor {
    pub editor: Entity<EditorState>,
    held: Option<Held>,
    is_dirty: bool,
    wants_focus: bool,
    /// Counts edits; only the last one's timer saves.
    serial: u64,
    /// Set when the held note was deleted: the next sync reloads it as empty.
    was_deleted: bool,
    failure: Option<String>,
    probe: Option<FolderProbe>,
    checked: HashSet<Location>,
    missing: Rc<HashSet<Location>>,
    _changes: Subscription,
}

impl NoteEditor {
    pub fn new(
        probe: Option<FolderProbe>,
        window: &mut Window,
        cx: &mut Context<ClipPalette>,
    ) -> Self {
        let editor = cx.new(|cx| plain_editor(window, cx));
        let changes = cx.subscribe(&editor, |palette, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                palette.note_changed(cx);
            }
        });
        Self {
            editor,
            held: None,
            is_dirty: false,
            wants_focus: false,
            serial: 0,
            was_deleted: false,
            failure: None,
            probe,
            checked: HashSet::new(),
            missing: Rc::default(),
            _changes: changes,
        }
    }

    /// Whether the editor holds the note of `scope`.
    pub fn holds(&self, scope: &Scope) -> bool {
        self.held.as_ref().is_some_and(|held| &held.scope == scope)
    }

    pub fn is_readable(&self) -> bool {
        self.held.as_ref().is_some_and(|held| held.is_readable)
    }

    /// Typed since the last save.
    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub fn ask_focus(&mut self) {
        self.wants_focus = true;
    }

    /// The held note was deleted: its edits are dropped and the next sync shows it empty.
    pub fn mark_deleted(&mut self) {
        self.is_dirty = false;
        self.was_deleted = true;
    }

    pub fn clear_probes(&mut self) {
        self.checked.clear();
        self.missing = Rc::default();
    }
}

/// A key the note editor takes while it has focus, instead of the palette.
#[derive(Clone, Copy)]
pub(super) enum Pass {
    Up,
    Down,
    Enter,
    Indent,
    Outdent,
    Copy,
    Paste,
    Delete,
}

impl Pass {
    pub(super) fn action(self) -> Box<dyn Action> {
        match self {
            Self::Up => Box::new(input::MoveUp),
            Self::Down => Box::new(input::MoveDown),
            Self::Enter => Box::new(input::Enter {
                secondary: false,
                shift: false,
            }),
            Self::Indent => Box::new(input::Indent),
            Self::Outdent => Box::new(input::Outdent),
            Self::Copy => Box::new(input::Copy),
            Self::Paste => Box::new(input::Paste),
            Self::Delete => Box::new(input::Delete),
        }
    }
}

impl ClipPalette {
    pub(super) fn missing_folders(&self) -> Rc<HashSet<Location>> {
        self.note.missing.clone()
    }

    fn note_changed(&mut self, cx: &mut Context<Self>) {
        if self.note.held.is_none() {
            return;
        }
        self.note.is_dirty = true;
        self.note.serial += 1;
        let serial = self.note.serial;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            let _ = this.update(cx, |palette, cx| palette.save_after_pause(serial, cx));
        })
        .detach();
        cx.notify();
    }

    fn save_after_pause(&mut self, serial: u64, cx: &mut Context<Self>) {
        if self.note.serial == serial {
            self.flush_note(cx);
            cx.notify();
        }
    }

    /// Saves the held note if it changed. A refusal stays in `failure` for the pane to say.
    pub(super) fn flush_note(&mut self, cx: &App) {
        let Some(held) = self.note.held.as_mut().filter(|held| held.is_readable) else {
            return;
        };
        let text = self.note.editor.read(cx).value().to_string();
        if text == held.baseline {
            self.note.is_dirty = false;
            return;
        }
        match self.hooks.set_note(held.scope.clone(), text.clone()) {
            Ok(()) => {
                held.baseline = text;
                self.note.is_dirty = false;
                self.note.failure = None;
            }
            Err(reason) => self.note.failure = Some(reason),
        }
    }

    /// The note the selected row stands for, and whether it is stored.
    fn wanted_note(&self) -> Option<(Scope, bool)> {
        let row = self.state.selected_row()?;
        let is_open = self.state.shown() == ClipMode::Notes && self.manage.is_none();
        let is_open = is_open && self.form.is_none();
        (is_open && row.kind == ClipKind::Note && !self.dormant)
            .then(|| (row.scope.clone(), !is_placeholder(row)))
    }

    fn load_note(&mut self, (scope, is_stored): (Scope, bool), window: &mut Window, cx: &mut App) {
        let text = match &scope {
            Scope::Folder(at) => self.hooks.note_for(at),
            Scope::Everywhere => self.hooks.global_note(),
        };
        let is_readable = text.is_some() || !is_stored;
        let text = text.unwrap_or_default();
        self.note.held = Some(Held {
            scope,
            baseline: text.clone(),
            is_readable,
        });
        self.note.is_dirty = false;
        self.note.failure = None;
        self.note.editor.update(cx, |editor, cx| {
            editor.set_value(text, window, cx);
            editor.set_readonly(!is_readable, cx);
        });
    }

    /// Drops the held note and its text from the editor.
    pub(super) fn forget_note(&mut self, window: &mut Window, cx: &mut App) {
        self.note.held = None;
        self.note.is_dirty = false;
        self.note.failure = None;
        self.note.editor.update(cx, |editor, cx| {
            editor.set_value(String::new(), window, cx);
            editor.set_readonly(false, cx);
        });
    }

    /// Brings the editor in step with the selected row. Render calls it, as it has the window.
    pub(super) fn sync_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.wanted_note();
        let held = self.note.held.as_ref().map(|held| &held.scope);
        let moved = wanted.as_ref().map(|(scope, _)| scope) != held;
        if moved {
            self.flush_note(cx);
            if let Some(reason) = self.note.failure.take() {
                self.say(t!("clip-note-not-saved", reason = reason), true, cx);
            }
            match wanted {
                Some(note) => self.load_note(note, window, cx),
                None => self.forget_note(window, cx),
            }
        } else if self.note.was_deleted
            && let Some((scope, _)) = wanted
        {
            self.load_note((scope, false), window, cx);
        }
        self.note.was_deleted = false;
        let wants_focus = std::mem::take(&mut self.note.wants_focus);
        if wants_focus && self.note.is_readable() {
            window.focus(&self.note.editor.focus_handle(cx), cx);
        }
    }

    pub(super) fn note_has_focus(&self, window: &Window, cx: &App) -> bool {
        self.note.editor.focus_handle(cx).is_focused(window)
    }

    /// Ctrl+E, and Enter on a note: the editor takes the keys once the note is loaded. Does
    /// nothing when the selected row is no note.
    pub fn focus_note(&mut self, cx: &mut Context<Self>) {
        self.note.ask_focus();
        cx.notify();
    }

    /// Esc in the editor: focus goes back to the search field. False when it was not there.
    pub(super) fn leave_note(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.note_has_focus(window, cx) {
            return false;
        }
        window.focus(&self.field.focus_handle(cx), cx);
        true
    }

    /// While the editor has focus the palette's keys belong to it; false when it does not.
    pub(super) fn pass_to_note(&self, key: Pass, window: &mut Window, cx: &mut App) -> bool {
        let has_focus = self.note_has_focus(window, cx);
        if has_focus {
            window.dispatch_action(key.action(), cx);
        }
        has_focus
    }

    /// Checks the folders of the listed notes off the UI thread and marks the missing ones.
    pub(super) fn probe_folders(&mut self, cx: &mut Context<Self>) {
        let Some(probe) = self.note.probe.clone() else {
            return;
        };
        let rows = self.state.results().rows.iter();
        let fresh: Vec<Location> = rows
            .filter(|row| row.kind == ClipKind::Note)
            .filter_map(|row| match &row.scope {
                Scope::Folder(at) => Some(at.clone()),
                Scope::Everywhere => None,
            })
            .filter(|at| Some(at) != self.state.folder() && !self.note.checked.contains(at))
            .take(PROBE_LIMIT)
            .collect();
        if fresh.is_empty() {
            return;
        }
        self.note.checked.extend(fresh.iter().cloned());
        let search = cx.background_spawn(async move {
            fresh
                .into_iter()
                .filter(|at| probe(at))
                .collect::<Vec<Location>>()
        });
        cx.spawn(async move |this, cx| {
            let gone = search.await;
            let _ = this.update(cx, |palette, cx| {
                Rc::make_mut(&mut palette.note.missing).extend(gone);
                cx.notify();
            });
        })
        .detach();
    }
}
