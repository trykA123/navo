use std::sync::Arc;

use rusty_core::{CommandError, LayoutError, Location, OpKind, StateError};

use super::FerriteWindow;
use super::shell_menu::put_paths_on_clipboard;
use crate::browser::{JobStatus, PasteRefusal};
use crate::format;
use crate::prelude::*;
use crate::status_bar::StatusNotice;
use rusty_l10n::t;

const NOTHING_TO_PASTE: &str = "Nothing to paste";
const WINDOWS_ONLY: &str = "The file clipboard works on Windows only";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardEffect {
    Copy,
    Move,
}

impl ClipboardEffect {
    fn kind(self) -> OpKind {
        match self {
            Self::Copy => OpKind::Copy,
            Self::Move => OpKind::Move,
        }
    }

    fn verb(self) -> &'static str {
        match self {
            Self::Copy => "Copied",
            Self::Move => "Cut",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFiles {
    pub items: Vec<Location>,
    pub effect: ClipboardEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardFault {
    Unsupported,
    Failed(String),
}

impl ClipboardFault {
    fn message(&self) -> SharedString {
        match self {
            Self::Unsupported => WINDOWS_ONLY.into(),
            Self::Failed(message) => message.clone().into(),
        }
    }
}

/// Calls may block for up to about 100 ms while another program holds the clipboard.
pub trait FileClipboardService: Send + Sync {
    fn write(&self, files: &ClipboardFiles) -> Result<(), ClipboardFault>;
    fn read(&self) -> Result<Option<ClipboardFiles>, ClipboardFault>;
    /// Never blocks: only asks whether files are offered, without reading them.
    fn has_files(&self) -> bool;
    fn clear_if_same(&self, files: &ClipboardFiles) -> Result<bool, ClipboardFault>;
}

#[derive(Default)]
pub(super) struct ClipboardState {
    service: Option<Arc<dyn FileClipboardService>>,
    notice: Option<StatusNotice>,
    cuts: Vec<(u64, ClipboardFiles)>,
}

impl ClipboardState {
    pub(super) fn new(service: Option<Arc<dyn FileClipboardService>>) -> Self {
        Self {
            service,
            ..Self::default()
        }
    }
}

fn info(text: impl Into<SharedString>) -> StatusNotice {
    StatusNotice {
        text: text.into(),
        is_error: false,
    }
}

fn refusal_text(refused: PasteRefusal) -> SharedString {
    match refused {
        PasteRefusal::Nothing => NOTHING_TO_PASTE.into(),
        PasteRefusal::AlreadyHere => "The files are already in this folder".into(),
        PasteRefusal::IntoItself => "Cannot paste a folder into itself".into(),
        PasteRefusal::IntoResults => crate::drag::Refusal::Results.text(),
    }
}

fn fault(fault: &ClipboardFault) -> StatusNotice {
    StatusNotice {
        text: fault.message(),
        is_error: true,
    }
}

impl FerriteWindow {
    fn file_clipboard(&self) -> Result<Arc<dyn FileClipboardService>, ClipboardFault> {
        self.clipboard
            .service
            .clone()
            .ok_or(ClipboardFault::Unsupported)
    }

    pub(super) fn clipboard_notice(&self) -> Option<StatusNotice> {
        self.clipboard.notice.clone()
    }

    pub(super) fn show_notice(&mut self, notice: StatusNotice) {
        self.clipboard.notice = Some(notice);
    }

    pub(super) fn show_refusal(&mut self, result: Result<(), CommandError>) {
        let key = match result {
            Err(CommandError::NothingToRestore) => "files-marks-restore-none",
            Err(CommandError::State(StateError::TooManyPanes)) => "layout-too-many-panes",
            Err(CommandError::State(StateError::InvalidTabLabel)) => "tab-label-too-long",
            Err(CommandError::State(StateError::WorkspaceLimit)) => "workspace-limit",
            Err(CommandError::State(StateError::Layout(LayoutError::LastPane))) => {
                "layout-last-pane"
            }
            _ => return,
        };
        self.show_notice(StatusNotice {
            text: rusty_l10n::tr(key).into(),
            is_error: true,
        });
    }

    pub(super) fn refuse_paste(&mut self, refused: PasteRefusal) {
        self.clipboard.notice = Some(info(refusal_text(refused)));
    }

    pub(super) fn forget_clipboard_notice(&mut self) {
        self.clipboard.notice = None;
    }

    pub(super) fn put_on_clipboard(&mut self, effect: ClipboardEffect) {
        let items = self.browser.selected_sources();
        if items.is_empty() {
            return;
        }
        let files = ClipboardFiles { items, effect };
        let service = self.file_clipboard();
        self.lane.push(Box::new(move || {
            let written = service.and_then(|service| service.write(&files));
            Box::new(move |view, _, _| view.clipboard_written(files, written))
        }));
    }

    fn clipboard_written(&mut self, files: ClipboardFiles, written: Result<(), ClipboardFault>) {
        let effect = files.effect;
        self.clipboard.notice = Some(match written {
            Ok(()) => {
                let count = u64::try_from(files.items.len()).unwrap_or(u64::MAX);
                let count = format::count(count, "item", "items");
                info(format!("{} {count} to the clipboard", effect.verb()))
            }
            Err(error) => fault(&error),
        });
    }

    pub(super) fn copy_full_paths(&mut self, _: &mut Context<Self>) {
        let items = self.browser.selected_sources();
        if items.is_empty() {
            return;
        }
        self.lane.push(Box::new(move || {
            Box::new(move |view, _, cx| {
                put_paths_on_clipboard(&items, cx);
                let count = u64::try_from(items.len()).unwrap_or(u64::MAX);
                view.clipboard.notice = Some(info(t!("files-path-copied", count = count)));
            })
        }));
    }

    pub(super) fn has_files_to_paste(&self) -> bool {
        self.file_clipboard()
            .is_ok_and(|service| service.has_files())
    }

    pub(super) fn paste(&mut self) {
        let service = self.file_clipboard();
        let target = self.browser.state().active_tab().location().clone();
        self.lane.push(Box::new(move || {
            let read = service.and_then(|service| service.read());
            Box::new(move |view, _, _| view.clipboard_read(target, read))
        }));
    }

    fn clipboard_read(
        &mut self,
        target: Location,
        read: Result<Option<ClipboardFiles>, ClipboardFault>,
    ) {
        let files = match read {
            Ok(Some(files)) if !files.items.is_empty() => files,
            Ok(_) => {
                self.clipboard.notice = Some(info(NOTHING_TO_PASTE));
                return;
            }
            Err(error) => {
                self.clipboard.notice = Some(fault(&error));
                return;
            }
        };
        self.clipboard.notice = None;
        let request = crate::browser::DropRequest {
            kind: files.effect.kind(),
            sources: files.items.clone(),
            target,
            confirm: crate::browser::DropConfirm::Never,
            source_tab: None,
        };
        match self.browser.drop_into(request) {
            Ok(id) if files.effect == ClipboardEffect::Move => {
                self.clipboard.cuts.push((id, files))
            }
            Ok(_) => {}
            Err(refused) => self.clipboard.notice = Some(info(refusal_text(refused))),
        }
    }

    pub(super) fn clear_finished_cuts(&mut self) {
        let jobs = self.browser.jobs();
        let status = |id: u64| jobs.iter().find(|job| job.id == id).map(|job| &job.status);
        let (ended, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.clipboard.cuts)
            .into_iter()
            .partition(|(id, _)| status(*id).is_some_and(|s| !s.is_active()));
        let done: Vec<ClipboardFiles> = ended
            .into_iter()
            .filter(|(id, _)| status(*id) == Some(&JobStatus::Done))
            .map(|(_, files)| files)
            .collect();
        self.clipboard.cuts = waiting;
        for files in done {
            let service = self.file_clipboard();
            self.lane.push(Box::new(move || {
                let cleared = service.and_then(|service| service.clear_if_same(&files));
                Box::new(move |view, _, _| view.clipboard_cleared(cleared))
            }));
        }
    }

    fn clipboard_cleared(&mut self, cleared: Result<bool, ClipboardFault>) {
        if let Err(error) = cleared {
            self.clipboard.notice = Some(fault(&error));
        }
    }
}
