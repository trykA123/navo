use std::pin::pin;
use std::time::Duration;

use futures::future::{Either, select};
use gpui_kit::Task;
use rusty_core::{Command, FsError, FsErrorKind, Location, PaneId, typed_location};
use rusty_fs::{EntryKind, FsEntry};

use super::FerriteWindow;
use crate::pane_chrome::PathInput;
use crate::prelude::*;

const CHECK_WAIT: Duration = Duration::from_secs(2);

pub(super) struct PathEdit {
    pub(super) pane: PaneId,
    pub(super) id: u64,
    error: Option<SharedString>,
    _check: Option<Task<()>>,
}

struct Checked {
    id: u64,
    location: Location,
    found: Option<Result<FsEntry, FsError>>,
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

impl FerriteWindow {
    pub(super) fn open_path_edit(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.drive_menu = None;
        self.browser.dialog_opened();
        self.next_prompt += 1;
        self.path_edit = Some(PathEdit {
            pane,
            id: self.next_prompt,
            error: None,
            _check: None,
        });
        cx.notify();
    }

    fn close_path_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.path_edit = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    fn submit_path(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &self.path_edit else {
            return;
        };
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
                view.path_checked(checked, window, cx)
            });
        });
        if let Some(edit) = &mut self.path_edit {
            edit._check = Some(task);
        }
    }

    fn path_checked(&mut self, checked: Checked, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.path_edit.as_ref().filter(|edit| edit.id == checked.id) else {
            return;
        };
        let pane = edit.pane;
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
        let (submit, cancel) = (cx.weak_entity(), cx.weak_entity());
        Some(
            PathInput::new(
                ("path-input", edit.id),
                location.path().display().into_owned(),
            )
            .error(edit.error.clone())
            .on_submit(move |value, window, cx| {
                let value = value.to_string();
                let _ = submit.update(cx, |view, cx| view.submit_path(&value, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.close_path_edit(window, cx));
            }),
        )
    }
}
