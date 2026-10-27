use std::rc::Rc;

use gpui_kit::{AnyWindowHandle, FocusHandle, WeakEntity};
use rusty_core::{Command, Location};

use super::body::{Attach, Host, OpenFile};
use super::editor::EditorBody;
use super::open::{Picked, load_or_text, load_while_alive, start};
use super::paged::PagedBody;
use super::{Kind, Slot, Viewer};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const SAVE_TO_FOLLOW: &str = "Save or discard your edits to follow this file.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pick {
    pub slot: Slot,
    pub edit: bool,
}

struct Choice {
    kind: Kind,
    edit: bool,
    follow_from: Option<u64>,
}

pub(super) struct Shown {
    pub picked: Picked,
    pub edit: bool,
}

impl Viewer {
    pub(in crate::window_view) fn focus_target(&self, cx: &App) -> FocusHandle {
        match &self.body {
            Some(body) => body.focus_handle(cx),
            None => self.focus.clone(),
        }
    }

    fn is_shown(&self, kind: &Kind) -> bool {
        self.body.is_some()
            && self
                .kind
                .as_ref()
                .is_some_and(|shown| shown.id() == kind.id())
    }

    fn switch_warning(&mut self, slot: Slot, cx: &mut App) -> bool {
        let Some(revision) = self.unsaved(cx) else {
            return false;
        };
        if self.switching == Some((slot, revision)) {
            return false;
        }
        self.switching = Some((slot, revision));
        let note = format!(
            "Unsaved changes. Ctrl+S saves; {} again discards them.",
            slot.alt_key()
        );
        self.say(note, cx);
        true
    }

    fn clear_body(&mut self) {
        self.body = None;
        self.memory = None;
        self.leaving = None;
        self.switching = None;
        self.message = None;
    }
}

impl FerriteWindow {
    fn host(&self, viewer: &Viewer, window: &Window, cx: &mut Context<Self>) -> Host {
        let this = cx.weak_entity();
        let open = Self::open_later(this.clone(), window.window_handle());
        Host {
            title: viewer.title.clone(),
            location: viewer.location.clone(),
            hooks: self.files.clone(),
            carried: viewer.carried.clone(),
            is_preview: false,
            on_saved: Rc::new(move |cx| {
                let this = this.clone();
                cx.defer(move |cx| {
                    let _ = this.update(cx, Self::refresh_after_save);
                });
            }),
            open,
        }
    }

    pub(in crate::window_view) fn open_later(
        this: WeakEntity<Self>,
        handle: AnyWindowHandle,
    ) -> OpenFile {
        Rc::new(move |location, cx| {
            let this = this.clone();
            cx.defer(move |cx| Self::open_in(&this, handle, location, cx));
        })
    }

    fn open_in(this: &WeakEntity<Self>, handle: AnyWindowHandle, location: Location, cx: &mut App) {
        let _ = cx.update_window(handle, |_, window, cx| {
            this.update(cx, |view, cx| view.view_location(location, window, cx))
        });
    }

    fn refresh_after_save(&mut self, cx: &mut Context<Self>) {
        let _ = self.browser.command(&Command::PaneRefresh);
        cx.notify();
    }

    pub(super) fn show_loaded(
        &mut self,
        shown: Shown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = &self.viewer else {
            return;
        };
        let host = self.host(viewer, window, cx);
        let Picked { kind, loaded, note } = shown.picked;
        let attached = loaded.and_then(|loaded| {
            let attach = Attach {
                loaded,
                host,
                edit: shown.edit,
            };
            kind.attach(attach, window, cx)
        });
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        viewer.opening = None;
        viewer.kind = Some(kind);
        viewer.carried = None;
        viewer.clear_body();
        match attached {
            Ok(body) => {
                viewer.body = Some(body);
                viewer.memory = viewer.kind.as_ref().map(|kind| {
                    super::hold_memory(&super::VIEWER_DOCUMENTS, kind, &viewer.sniff(), cx)
                });
            }
            Err(message) => viewer.message = Some(message.into()),
        }
        if let (Some(line), Some(body)) = (viewer.start_line.take(), &viewer.body) {
            body.go_to_line(line, window, cx);
        }
        if let Some(note) = super::merged_note(note, viewer.start_note.take()) {
            viewer.say(note, cx);
        }
        viewer.note_detected(cx);
        window.focus(&viewer.focus_target(cx), cx);
        self.show_parts(cx);
        cx.notify();
    }

    pub(super) fn show_slot(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        let kind = if pick.edit {
            viewer.editable_kind()
        } else {
            viewer.next_in(pick.slot)
        };
        let Some(kind) = kind else {
            return;
        };
        if viewer.is_shown(&kind) || viewer.switch_warning(pick.slot, cx) {
            cx.notify();
            return;
        }
        let reused = match &viewer.body {
            Some(body) => kind.reuse(body.as_ref(), &viewer.sniff(), cx),
            None => false,
        };
        if reused {
            viewer.kind = Some(kind);
            window.focus(&viewer.focus_target(cx), cx);
            self.show_parts(cx);
            cx.notify();
            return;
        }
        let choice = Choice {
            kind,
            edit: pick.edit,
            follow_from: None,
        };
        self.load_kind(choice, window, cx);
    }

    pub(super) fn reload_kind(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.viewer.as_ref().and_then(|viewer| viewer.kind.clone()) else {
            return;
        };
        let choice = Choice {
            kind,
            edit: false,
            follow_from: None,
        };
        self.load_kind(choice, window, cx);
    }

    fn load_kind(&mut self, choice: Choice, window: &mut Window, cx: &mut Context<Self>) {
        let Choice {
            kind,
            edit,
            follow_from,
        } = choice;
        let Some(viewer) = &self.viewer else {
            return;
        };
        let request = self.request(viewer);
        let sniffed = (viewer.head.clone(), viewer.title.to_string());
        let kinds = viewer.kinds.clone();
        let opening = start(
            window,
            cx,
            move |turn| {
                let sniff = super::Sniff {
                    head: &sniffed.0.bytes,
                    name: &sniffed.1,
                    len: sniffed.0.len,
                };
                match follow_from {
                    Some(_) => Picked {
                        loaded: load_while_alive(turn.alive, &kind, || request.paged_text()),
                        kind,
                        note: None,
                    },
                    None => load_or_text(&kinds, kind, request.loader(&sniff, turn)),
                }
            },
            move |view: &mut Self, picked, window, cx| {
                if view.restart_if_superseded(window, cx) {
                    return;
                }
                view.show_loaded(Shown { picked, edit }, window, cx);
                if let Some(line) = follow_from {
                    view.follow_shown_from(line, cx);
                }
            },
        );
        if let Some(viewer) = &mut self.viewer {
            if let Some(carried) = viewer.body.as_ref().and_then(|body| body.carry(cx)) {
                viewer.carried = Some(carried);
            }
            viewer.clear_body();
            viewer.opening = Some(opening);
            window.focus(&viewer.focus, cx);
        }
        cx.notify();
    }

    pub(super) fn follow_from_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(viewer) = &mut self.viewer else {
            return false;
        };
        let Some(body) = viewer.body.as_ref() else {
            return false;
        };
        if body.is_dirty(cx) {
            viewer.say(SAVE_TO_FOLLOW, cx);
            cx.notify();
            return true;
        }
        let editor = body
            .as_any()
            .downcast_ref::<EditorBody>()
            .filter(|_| !body.is_editing(cx));
        let (Some(editor), Some(kind)) = (editor, viewer.kind.clone()) else {
            return false;
        };
        let line = editor.0.read(cx).cursor_row(cx) + 1;
        let choice = Choice {
            kind,
            edit: false,
            follow_from: Some(line),
        };
        self.load_kind(choice, window, cx);
        true
    }

    fn follow_shown_from(&mut self, line: u64, cx: &mut Context<Self>) {
        let paged = self
            .viewer
            .as_ref()
            .and_then(|viewer| viewer.body.as_ref())
            .and_then(|body| PagedBody::of(body.as_ref()))
            .map(|paged| paged.0.clone());
        if let Some(paged) = paged {
            paged.update(cx, |paged, cx| paged.follow_from_line(line, cx));
        }
    }
}
