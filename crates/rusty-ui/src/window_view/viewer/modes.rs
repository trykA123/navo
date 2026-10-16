use std::rc::Rc;

use gpui_kit::FocusHandle;
use rusty_core::Command;

use super::body::{Attach, Host};
use super::editor::EditorBody;
use super::kinds::paged_text;
use super::open::{load_while_alive, start};
use super::paged::PagedBody;
use super::{Kind, Loaded, Slot, Viewer};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

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
    pub kind: Kind,
    pub loaded: Result<Loaded, String>,
    pub edit: bool,
}

impl Viewer {
    pub(super) fn focus_target(&self, cx: &App) -> FocusHandle {
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
        self.leaving = None;
        self.switching = None;
        self.message = None;
    }
}

impl FerriteWindow {
    fn host(&self, viewer: &Viewer, cx: &mut Context<Self>) -> Host {
        let this = cx.weak_entity();
        Host {
            title: viewer.title.clone(),
            location: viewer.location.clone(),
            hooks: self.files.clone(),
            carried: viewer.carried.clone(),
            on_saved: Rc::new(move |cx| {
                let this = this.clone();
                cx.defer(move |cx| {
                    let _ = this.update(cx, Self::refresh_after_save);
                });
            }),
        }
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
        let host = self.host(viewer, cx);
        let attached = shown.loaded.and_then(|loaded| {
            let attach = Attach {
                loaded,
                host,
                edit: shown.edit,
            };
            shown.kind.attach(attach, window, cx)
        });
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        viewer.opening = None;
        viewer.kind = Some(shown.kind);
        viewer.carried = None;
        viewer.clear_body();
        match attached {
            Ok(body) => viewer.body = Some(body),
            Err(message) => viewer.message = Some(message.into()),
        }
        window.focus(&viewer.focus_target(cx), cx);
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
        let loader = kind.clone();
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
                    Some(_) => load_while_alive(turn.alive, &loader, || {
                        (request.hooks.paged)(&request.location).map(paged_text)
                    }),
                    None => request.load(&loader, &sniff, turn),
                }
            },
            move |view: &mut Self, loaded, window, cx| {
                if view.restart_if_superseded(window, cx) {
                    return;
                }
                view.show_loaded(Shown { kind, loaded, edit }, window, cx);
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
        let Some(viewer) = &self.viewer else {
            return false;
        };
        let editor = viewer
            .body
            .as_ref()
            .and_then(|body| body.as_any().downcast_ref::<EditorBody>())
            .filter(|editor| !editor.0.read(cx).is_dirty());
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
