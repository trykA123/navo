use std::sync::Arc;

use gpui_kit::{DragMoveEvent, Focusable, Subscription};
use navo_core::{Command, FsErrorKind, Location};
use navo_fs::FsEntry;
use navo_l10n::t;

use super::path_edit::{is_folder, last_name};
use super::{ClipboardSummon, NavoWindow};
use crate::clip_palette::{
    ClipCounts, ClipPalette, ClipPaletteEvent, ClipPaletteProps, ClipboardHooks, DraggedPalette,
    FolderProbe, Place, PopUpRequest, clamp_offset, window_panel,
};
use crate::commands::{CommandEntry, entries_with};
use crate::confirm_dialog::{ConfirmDialog, ConfirmProps};
use crate::prelude::*;
use crate::settings::{Change, Field, Value};
use crate::status_bar::StatusNotice;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ask {
    Clear(ClipCounts),
    DeleteAll(ClipCounts),
}

/// A palette drag under way: where the pointer and the offset were at the press.
#[derive(Clone, Copy)]
struct Drag {
    from: Point<Pixels>,
    base: Point<Pixels>,
}

struct Open {
    palette: Entity<ClipPalette>,
    _events: Subscription,
}

/// The main-window clipboard palette and the confirms of its two destructive commands.
#[derive(Default)]
pub(super) struct ClipboardPanel {
    hooks: Option<Arc<dyn ClipboardHooks>>,
    open: Option<Open>,
    ask: Option<Ask>,
    asks: u64,
    /// How far the palette sits from the window centre; kept for the run, not saved.
    offset: Point<Pixels>,
    drag: Option<Drag>,
}

impl ClipboardPanel {
    pub(super) fn new(hooks: Option<Arc<dyn ClipboardHooks>>) -> Self {
        Self {
            hooks,
            ..Self::default()
        }
    }

    /// The hooks, unless this build has no clipboard history.
    pub(super) fn hooks(&self) -> Option<&Arc<dyn ClipboardHooks>> {
        self.hooks.as_ref().filter(|hooks| hooks.counts().is_some())
    }

    /// The hooks while the history is on: the commands exist only then (D21a). The palette
    /// itself opens off too, to say why it is empty.
    pub(super) fn live(&self) -> Option<&Arc<dyn ClipboardHooks>> {
        self.hooks().filter(|hooks| !hooks.modes().is_empty())
    }

    #[cfg(test)]
    pub(super) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    #[cfg(test)]
    pub(super) fn offset(&self) -> Point<Pixels> {
        self.offset
    }

    #[cfg(test)]
    pub(super) fn asking(&self) -> bool {
        self.ask.is_some()
    }
}

fn confirm(title: String, lines: Vec<String>, summary: String, label: String) -> ConfirmProps {
    ConfirmProps {
        title: title.into(),
        lines: lines.into_iter().map(Into::into).collect(),
        more: 0,
        summary: summary.into(),
        confirm_label: label.into(),
        danger: true,
        typed: None,
        mode: None,
        choice: None,
    }
}

pub(super) fn ask_props(ask: Ask) -> ConfirmProps {
    match ask {
        Ask::Clear(counts) => confirm(
            t!("clip-clear-title"),
            vec![t!("clip-clear-count", count = i64::from(counts.history))],
            t!("clip-clear-summary"),
            t!("clip-clear-confirm"),
        ),
        Ask::DeleteAll(counts) => confirm(
            t!("clip-wipe-title"),
            vec![
                t!("clip-wipe-history", count = i64::from(counts.history)),
                t!("clip-wipe-saved", count = i64::from(counts.saved)),
                t!("clip-wipe-notes", count = i64::from(counts.notes)),
            ],
            t!("clip-wipe-summary"),
            t!("clip-wipe-confirm"),
        ),
    }
}

enum Reach {
    File(Location, String),
    Folder(Location),
}

fn reach_of(location: Location, found: Option<FsEntry>) -> Reach {
    let file = found.filter(|entry| !is_folder(entry));
    match (file.and(last_name(&location)), location.parent()) {
        (Some(name), Some(folder)) => Reach::File(folder, name),
        _ => Reach::Folder(location),
    }
}

fn notice(text: String, is_error: bool) -> StatusNotice {
    StatusNotice {
        text: text.into(),
        is_error,
    }
}

impl NavoWindow {
    /// Finds a note's folder that is gone. Only a plain "not found" counts: an offline share
    /// or a denied folder may be back.
    fn folder_probe(&self) -> FolderProbe {
        let provider = self.provider.clone();
        Arc::new(move |folder| {
            provider
                .metadata(folder)
                .is_err_and(|error| error.kind == FsErrorKind::NotFound)
        })
    }

    /// The commands Ctrl+K lists; the clipboard ones need clipboard hooks.
    pub(super) fn command_entries(&self) -> Vec<CommandEntry> {
        entries_with(self.clip.live().is_some())
    }

    pub(super) fn open_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer.is_some() {
            cx.propagate();
            return;
        }
        let Some(hooks) = self.clip.hooks().cloned() else {
            return;
        };
        if let Some(open) = &self.clip.open {
            window.focus(&open.palette.focus_handle(cx), cx);
            return;
        }
        let is_busy = self.prompt.is_some()
            || self.browser.pending().is_some()
            || self.is_multi_rename_open()
            || self.settings.is_open()
            || self.recent.is_open()
            || self.clip.ask.is_some();
        if is_busy {
            return;
        }
        self.close_command_center(window, cx);
        if self.app_menu.is_some() {
            self.close_app_menu(window, cx);
        }
        self.drive_menu = None;
        self.path_edit = None;
        self.browser.dialog_opened();
        let folder = Some(self.browser.state().active_tab().location().clone());
        let key_cap = crate::commands::active(cx)
            .keys_of("clipboard.open")
            .next()
            .map(str::to_owned);
        let props = ClipPaletteProps {
            hooks,
            place: Place::Window,
            folder,
            key_cap: key_cap.map(Into::into),
            now_ms: None,
            width: None,
            folder_probe: Some(self.folder_probe()),
        };
        let palette = cx.new(|cx| ClipPalette::new(props, window, cx));
        let events = cx.subscribe_in(&palette, window, |view, _, event, window, cx| {
            view.clipboard_palette_event(event, window, cx);
        });
        self.clip.open = Some(Open {
            palette,
            _events: events,
        });
        cx.notify();
    }

    /// The clipboard key opens the pop-up over the app in front; this window stays as it was.
    pub(super) fn summon_clipboard(&mut self, summon: ClipboardSummon, cx: &mut Context<Self>) {
        let Some(hooks) = self.clip.hooks().cloned() else {
            return;
        };
        let key = self.values.clipboard_key().to_lowercase().replace('+', "-");
        let request = PopUpRequest {
            summon,
            hooks,
            key_cap: Some(key.into()),
            folder_probe: Some(self.folder_probe()),
            window_shown: self.window_shown.clone(),
            position: self.values.popup_position(),
        };
        cx.defer(move |cx| crate::clip_palette::summon(request, cx));
    }

    pub(super) fn close_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clip.open.take().is_some() {
            self.focus_core_pane(window, cx);
            cx.notify();
        }
    }

    fn clipboard_palette_event(
        &mut self,
        event: &ClipPaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ClipPaletteEvent::Dismissed | ClipPaletteEvent::PasteBack => {
                self.close_clipboard(window, cx);
            }
            ClipPaletteEvent::DragStart(from) => {
                let base = self.clip.offset;
                self.clip.drag = Some(Drag { from: *from, base });
            }
            ClipPaletteEvent::GoTo(location) => {
                self.close_clipboard(window, cx);
                self.go_to_clipboard_path(location.clone(), window, cx);
            }
            ClipPaletteEvent::PasteIntoPane => {
                self.close_clipboard(window, cx);
                self.paste();
            }
        }
    }

    /// Opens a folder in the active pane; a file opens its folder with the file under the cursor.
    fn go_to_clipboard_path(
        &mut self,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let provider = self.provider.clone();
        cx.spawn_in(window, async move |this, cx| {
            let target = location.clone();
            let found = cx
                .background_spawn(async move { provider.metadata(&target) })
                .await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.reach_clipboard_path(reach_of(location, found.ok()), window, cx);
            });
        })
        .detach();
    }

    fn reach_clipboard_path(&mut self, reach: Reach, window: &mut Window, cx: &mut Context<Self>) {
        match reach {
            Reach::File(folder, name) => {
                self.browser.open_at(folder, name);
                cx.notify();
            }
            Reach::Folder(location) => self.apply(&Command::PaneNavigate { location }, window, cx),
        }
    }

    pub(super) fn pause_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clip.live().is_none() {
            return;
        }
        let record = !self.values.records_clipboard();
        let change = Change {
            field: Field::ClipboardRecord,
            value: Value::Flag(record),
        };
        self.change_setting(change, window, cx);
        let key = if record {
            "clip-pause-off"
        } else {
            "clip-pause-on"
        };
        self.show_notice(notice(t!(key), false));
        cx.notify();
    }

    pub(super) fn ask_clipboard(
        &mut self,
        delete_all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(counts) = self.clip.live().and_then(|hooks| hooks.counts()) else {
            return;
        };
        self.close_clipboard(window, cx);
        self.browser.dialog_opened();
        self.clip.asks += 1;
        self.clip.ask = Some(if delete_all {
            Ask::DeleteAll(counts)
        } else {
            Ask::Clear(counts)
        });
        cx.notify();
    }

    pub(super) fn answer_clipboard_ask(
        &mut self,
        confirmed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ask = self.clip.ask.take();
        self.focus_core_pane(window, cx);
        if let (true, Some(ask), Some(hooks)) = (confirmed, ask, self.clip.hooks()) {
            let (outcome, done) = match ask {
                Ask::Clear(_) => (hooks.clear_history(), "clip-clear-done"),
                Ask::DeleteAll(_) => (hooks.delete_all(), "clip-wipe-done"),
            };
            let shown = match outcome {
                Ok(()) => notice(t!(done), false),
                Err(error) => notice(error, true),
            };
            self.show_notice(shown);
        }
        cx.notify();
    }

    pub(super) fn clipboard_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let margin = crate::tokens::current(cx).spacing.md;
        let panel = window_panel(window.viewport_size());
        let palette = self.clip.open.as_ref().map(|open| {
            let at = clamp_offset(self.clip.offset, panel, window.viewport_size(), margin);
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .on_drag_move(cx.listener(
                    move |view, event: &DragMoveEvent<DraggedPalette>, window, cx| {
                        view.drag_clipboard_palette(event.event.position, window, cx);
                    },
                ))
                .child(
                    div()
                        .relative()
                        .left(at.x)
                        .top(at.y)
                        .child(open.palette.clone()),
                )
                .into_any_element()
        });
        palette.into_iter().chain(self.clipboard_ask(cx)).collect()
    }

    fn drag_clipboard_palette(
        &mut self,
        pointer: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.clip.drag else {
            return;
        };
        let margin = crate::tokens::current(cx).spacing.md;
        let viewport = window.viewport_size();
        let moved = drag.base + (pointer - drag.from);
        self.clip.offset = clamp_offset(moved, window_panel(viewport), viewport, margin);
        cx.notify();
    }

    fn clipboard_ask(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ask = self.clip.ask?;
        let (confirm, cancel) = (cx.weak_entity(), cx.weak_entity());
        let dialog = ConfirmDialog::new(("clipboard-ask", self.clip.asks), ask_props(ask))
            .on_confirm(move |window, cx| {
                let _ = confirm.update(cx, |view, cx| view.answer_clipboard_ask(true, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.answer_clipboard_ask(false, window, cx));
            });
        Some(
            div()
                .id("clipboard-ask-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(crate::tokens::current(cx).surfaces.scrim)
                .occlude()
                .child(dialog)
                .into_any_element(),
        )
    }
}
