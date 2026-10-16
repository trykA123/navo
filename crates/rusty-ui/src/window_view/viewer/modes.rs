use std::rc::Rc;

use gpui_kit::FocusHandle;
use rusty_core::Command;

use super::body::{Attach, Host};
use super::open::{load_while_alive, start};
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
        };
        self.load_kind(choice, window, cx);
    }

    fn load_kind(&mut self, choice: Choice, window: &mut Window, cx: &mut Context<Self>) {
        let Choice { kind, edit } = choice;
        let Some(viewer) = &self.viewer else {
            return;
        };
        let request = self.request(viewer);
        let sniffed = (viewer.head.clone(), viewer.title.to_string());
        let loader = kind.clone();
        let opening = start(
            window,
            cx,
            move |alive| {
                let sniff = super::Sniff {
                    head: &sniffed.0.bytes,
                    name: &sniffed.1,
                    len: sniffed.0.len,
                };
                load_while_alive(alive, &loader, || {
                    loader.load(&request.hooks, &request.location, &sniff)
                })
            },
            move |view: &mut Self, loaded, window, cx| {
                view.show_loaded(Shown { kind, loaded, edit }, window, cx);
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
}
