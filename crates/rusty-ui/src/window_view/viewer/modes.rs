use std::rc::Rc;

use gpui_kit::{AnyWindowHandle, FocusHandle, WeakEntity};
use rusty_core::{Command, Location};

use super::body::{Attach, Host, OpenFile};
use super::open::{Picked, load_or_text, start};
use super::{Kind, Slot, Viewer};
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
    pub picked: Picked,
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
    fn host(&self, viewer: &Viewer, window: &Window, cx: &mut Context<Self>) -> Host {
        let this = cx.weak_entity();
        let open = Self::open_later(this.clone(), window.window_handle());
        Host {
            title: viewer.title.clone(),
            location: viewer.location.clone(),
            hooks: self.files.clone(),
            on_saved: Rc::new(move |cx| {
                let this = this.clone();
                cx.defer(move |cx| {
                    let _ = this.update(cx, Self::refresh_after_save);
                });
            }),
            open,
        }
    }

    fn open_later(this: WeakEntity<Self>, handle: AnyWindowHandle) -> OpenFile {
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
        viewer.clear_body();
        match attached {
            Ok(body) => viewer.body = Some(body),
            Err(message) => viewer.message = Some(message.into()),
        }
        if let Some(note) = note {
            viewer.say(note, cx);
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
        let kinds = viewer.kinds.clone();
        let opening = start(
            window,
            cx,
            move |alive| {
                let sniff = super::Sniff {
                    head: &sniffed.0.bytes,
                    name: &sniffed.1,
                    len: sniffed.0.len,
                };
                load_or_text(&kinds, kind, request.loader(&sniff, alive))
            },
            move |view: &mut Self, picked, window, cx| {
                view.show_loaded(Shown { picked, edit }, window, cx);
            },
        );
        if let Some(viewer) = &mut self.viewer {
            viewer.clear_body();
            viewer.opening = Some(opening);
            window.focus(&viewer.focus, cx);
        }
        cx.notify();
    }
}
