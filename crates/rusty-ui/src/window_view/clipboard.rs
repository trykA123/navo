use std::rc::Rc;

use rusty_core::{Location, OpKind};

use super::FerriteWindow;
use crate::browser::{JobStatus, PasteRefusal};
use crate::format;
use crate::prelude::*;
use crate::status_bar::StatusNotice;

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
pub trait FileClipboardService {
    fn write(&self, files: &ClipboardFiles) -> Result<(), ClipboardFault>;
    fn read(&self) -> Result<Option<ClipboardFiles>, ClipboardFault>;
    fn clear_if_same(&self, files: &ClipboardFiles) -> Result<bool, ClipboardFault>;
}

#[derive(Default)]
pub(super) struct ClipboardState {
    service: Option<Rc<dyn FileClipboardService>>,
    notice: Option<StatusNotice>,
    cuts: Vec<(u64, ClipboardFiles)>,
}

impl ClipboardState {
    pub(super) fn new(service: Option<Rc<dyn FileClipboardService>>) -> Self {
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

fn refusal_text(refused: PasteRefusal) -> &'static str {
    match refused {
        PasteRefusal::Nothing => NOTHING_TO_PASTE,
        PasteRefusal::AlreadyHere => "The files are already in this folder",
        PasteRefusal::IntoItself => "Cannot paste a folder into itself",
    }
}

fn fault(fault: &ClipboardFault) -> StatusNotice {
    StatusNotice {
        text: fault.message(),
        is_error: true,
    }
}

impl FerriteWindow {
    fn file_clipboard(&self) -> Result<Rc<dyn FileClipboardService>, ClipboardFault> {
        self.clipboard
            .service
            .clone()
            .ok_or(ClipboardFault::Unsupported)
    }

    pub(super) fn clipboard_notice(&self) -> Option<StatusNotice> {
        self.clipboard.notice.clone()
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
        let written = self
            .file_clipboard()
            .and_then(|service| service.write(&files));
        self.clipboard.notice = Some(match written {
            Ok(()) => {
                let count = u64::try_from(files.items.len()).unwrap_or(u64::MAX);
                let count = format::count(count, "item", "items");
                info(format!("{} {count} to the clipboard", effect.verb()))
            }
            Err(error) => fault(&error),
        });
    }

    pub(super) fn paste(&mut self) {
        let read = self.file_clipboard().and_then(|service| service.read());
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
        match self.browser.paste(files.effect.kind(), files.items.clone()) {
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
            .partition(|(id, _)| status(*id).is_some_and(|s| *s != JobStatus::Running));
        let done: Vec<ClipboardFiles> = ended
            .into_iter()
            .filter(|(id, _)| status(*id) == Some(&JobStatus::Done))
            .map(|(_, files)| files)
            .collect();
        self.clipboard.cuts = waiting;
        for files in done {
            let cleared = self
                .file_clipboard()
                .and_then(|service| service.clear_if_same(&files));
            if let Err(error) = cleared {
                self.clipboard.notice = Some(fault(&error));
            }
        }
    }
}
