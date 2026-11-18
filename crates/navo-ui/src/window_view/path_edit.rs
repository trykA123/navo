use std::pin::pin;
use std::time::Duration;

use futures::future::{Either, select};
use gpui_kit::Task;
use navo_core::{Command, FsError, FsErrorKind, Location, PaneId, TabId, typed_location};
use navo_fs::{EntryKind, FsEntry};

use super::NavoWindow;
use crate::pane_chrome::{InFocus, PathInput};
use crate::prelude::*;

const CHECK_WAIT: Duration = Duration::from_secs(2);

pub(super) struct PathEdit {
    pub(super) pane: PaneId,
    pub(super) id: u64,
    tab: TabId,
    location: Location,
    error: Option<SharedString>,
    in_focus: Option<InFocus>,
    recheck: bool,
    _check: Option<Task<()>>,
}

/// The answer of a typed-path check: the edit it belongs to, the target and what was found.
pub(super) struct Checked {
    pub(super) id: u64,
    pub(super) location: Location,
    pub(super) found: Option<Result<FsEntry, FsError>>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

pub(super) fn last_name(location: &Location) -> Option<String> {
    location
        .path()
        .to_text()
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())
        .map(str::to_owned)
}

pub(super) fn is_folder(entry: &FsEntry) -> bool {
    entry.kind == EntryKind::Folder
        || (entry.kind == EntryKind::Link && entry.attributes.link_to_folder)
}

impl NavoWindow {
    pub(super) fn open_path_edit(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.drive_menu = None;
        self.browser.dialog_opened();
        self.next_prompt += 1;
        let Some(active) = self.browser.state().pane(pane).map(|pane| pane.active()) else {
            return;
        };
        let (tab, location) = (active.id(), active.location().clone());
        self.path_edit = Some(PathEdit {
            pane,
            id: self.next_prompt,
            tab,
            location,
            error: None,
            in_focus: None,
            recheck: false,
            _check: None,
        });
        cx.notify();
    }

    fn close_path_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.path_edit = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    /// Closes edit `id` when its field lost focus, keeping focus where the user put it.
    /// Ignored for an older edit and while the window is inactive, so Alt+Tab does not close it.
    fn path_blurred(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let is_this = self.path_edit.as_ref().is_some_and(|edit| edit.id == id);
        if is_this && window.is_window_active() {
            self.path_edit = None;
            cx.notify();
        }
    }

    /// Asks the next active frame to recheck the field's focus: GPUI reports no blur for a
    /// click that moved focus before the first frame after the window became active.
    pub(super) fn recheck_path_focus(&mut self) {
        if let Some(edit) = &mut self.path_edit {
            edit.recheck = true;
        }
    }

    /// Closes the edit at the first active frame after activation when focus left its field
    /// and menu, keeping focus where the click put it. Runs during render, before the panes.
    pub(super) fn sync_path_edit(&mut self, window: &Window, cx: &App) {
        let Some(edit) = self.path_edit.as_mut().filter(|edit| edit.recheck) else {
            return;
        };
        if !window.is_window_active() {
            return;
        }
        edit.recheck = false;
        if edit
            .in_focus
            .as_ref()
            .is_some_and(|in_focus| !in_focus(window, cx))
        {
            self.path_edit = None;
        }
    }

    /// Keeps the focus probe of edit `id` once its field took focus.
    fn path_focused(&mut self, id: u64, in_focus: InFocus) {
        if let Some(edit) = self.path_edit.as_mut().filter(|edit| edit.id == id) {
            edit.in_focus = Some(in_focus);
        }
    }

    /// True when the edit's pane shows another tab or location than it opened on.
    fn has_moved(&self, edit: &PathEdit) -> bool {
        let state = self.browser.state();
        state
            .pane(edit.pane)
            .map(|pane| pane.active())
            .is_none_or(|active| active.id() != edit.tab || *active.location() != edit.location)
    }

    /// True when edit `id` is open on its first tab and location, and focus is still in its field.
    fn is_live_edit(&self, id: u64, window: &Window, cx: &App) -> bool {
        self.path_edit
            .as_ref()
            .filter(|edit| edit.id == id && !self.has_moved(edit))
            .and_then(|edit| edit.in_focus.as_ref())
            .is_some_and(|in_focus| in_focus(window, cx))
    }

    /// Closes the edit when its pane shows another tab or location than it opened on.
    pub(super) fn close_moved_path_edit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = &self.path_edit else {
            return;
        };
        if self.has_moved(edit) {
            self.path_edit = None;
            cx.notify();
        }
    }

    /// Checks a submitted path and keeps the probe that tells whether focus is still in the field.
    fn submit_path(
        &mut self,
        (value, in_focus): (&str, InFocus),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.path_edit else {
            return;
        };
        edit.in_focus = Some(in_focus);
        let Some(current) = self
            .browser
            .state()
            .pane(edit.pane)
            .map(|pane| pane.active().location().clone())
        else {
            return;
        };
        match typed_location(value, &current, &env) {
            Err(message) => self.path_error(message.into(), cx),
            Ok(location) => self.check_path(location, window, cx),
        }
    }

    fn path_error(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.path_edit {
            edit.error = Some(message);
        }
        cx.notify();
    }

    fn check_path(&mut self, location: Location, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &self.path_edit else {
            return;
        };
        let id = edit.id;
        let provider = self.provider.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let target = location.clone();
            let lookup = cx.background_spawn(async move { provider.metadata(&target) });
            let timeout = cx.background_executor().timer(CHECK_WAIT);
            let found = match select(pin!(lookup), pin!(timeout)).await {
                Either::Left((found, _)) => Some(found),
                Either::Right(_) => None,
            };
            let checked = Checked {
                id,
                location,
                found,
            };
            let _ = this.update_in(cx, |view, window, cx| {
                view.path_checked(checked, window, cx);
            });
        });
        if let Some(edit) = &mut self.path_edit {
            edit._check = Some(task);
        }
    }

    /// Applies a check result, unless the user left the edit, its tab or its location meanwhile.
    pub(super) fn path_checked(
        &mut self,
        checked: Checked,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self
            .path_edit
            .as_ref()
            .filter(|edit| edit.id == checked.id)
            .map(|edit| edit.pane)
        else {
            return;
        };
        if !self.is_live_edit(checked.id, window, cx) {
            self.path_edit = None;
            cx.notify();
            return;
        }
        let Checked {
            location, found, ..
        } = checked;
        let file = match found {
            Some(Err(error)) if error.kind == FsErrorKind::NotFound => {
                return self.path_error("Not found".into(), cx);
            }
            Some(Ok(entry)) if !is_folder(&entry) => last_name(&location),
            _ => None,
        };
        self.path_edit = None;
        self.focus_pane(pane);
        match (file, location.parent()) {
            (Some(name), Some(folder)) => {
                self.browser.open_at(folder, name);
                self.focus_core_pane(window, cx);
                cx.notify();
            }
            _ => self.apply(&Command::PaneNavigate { location }, window, cx),
        }
    }

    pub(super) fn path_input(&self, pane: PaneId, cx: &mut Context<Self>) -> Option<PathInput> {
        let edit = self.path_edit.as_ref().filter(|edit| edit.pane == pane)?;
        let location = self.browser.state().pane(pane)?.active().location().clone();
        let (submit, cancel, blur) = (cx.weak_entity(), cx.weak_entity(), cx.weak_entity());
        let focused = cx.weak_entity();
        let id = edit.id;
        Some(
            PathInput::new(
                ("path-input", edit.id),
                location.path().display().into_owned(),
            )
            .error(edit.error.clone())
            .on_submit(move |value, in_focus, window, cx| {
                let value = value.to_string();
                let _ = submit.update(cx, |view, cx| {
                    view.submit_path((&value, in_focus), window, cx);
                });
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.close_path_edit(window, cx));
            })
            .on_blur(move |window, cx| {
                let _ = blur.update(cx, |view, cx| view.path_blurred(id, window, cx));
            })
            .on_focused(move |in_focus, cx| {
                let _ = focused.update(cx, |view, _| view.path_focused(id, in_focus));
            }),
        )
    }
}
