use rusty_core::Location;
use std::sync::Arc;

use rusty_core::encoding::TextEncoding;
use rusty_core::line_ends::LineEnds;

use super::EditorView;
use crate::prelude::*;
use crate::window_view::viewer::search::memory::restamp;
use crate::window_view::viewer::{FileHooks, FileStamp, Save, SaveError};

const SAVING: &str = "Saving…";
const SAVED: &str = "Saved.";
const CHANGED_WARNING: &str = "Changed on disk since it was opened. Ctrl+S again overwrites it.";

struct SaveJob {
    location: Location,
    text: String,
    crlf: bool,
    encoding: TextEncoding,
    ends: Option<Arc<LineEnds>>,
    expected: Option<FileStamp>,
}

struct Written {
    result: Result<Option<FileStamp>, SaveError>,
    ends: Option<Arc<LineEnds>>,
}

impl SaveJob {
    fn write(&self, hooks: &FileHooks) -> Written {
        let save = Save {
            text: &self.text,
            crlf: self.crlf,
            expected: self.expected,
            encoding: self.encoding,
            ends: self.ends.as_deref(),
        };
        let result = (hooks.write)(&self.location, &save);
        let ends = match (&result, &self.ends) {
            (Ok(_), Some(ends)) => Some(Arc::new(ends.rebuilt(&self.text))),
            _ => None,
        };
        Written { result, ends }
    }
}

impl EditorView {
    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        if !self.editing || self.saving.is_some() {
            return;
        }
        let job = SaveJob {
            location: self.host.location.clone(),
            text: self.editor.read(cx).value().to_string(),
            crlf: self.crlf,
            encoding: self.encoding,
            ends: self.ends.clone(),
            expected: if self.overwrite { None } else { self.stamp },
        };
        let hooks = self.host.hooks.clone();
        let revision = self.revision;
        self.overwrite = false;
        self.note = Some(SAVING.into());
        let (sender, receiver) = futures::channel::oneshot::channel();
        cx.background_spawn(async move {
            let _ = sender.send(job.write(&hooks));
        })
        .detach();
        self.saving = Some(cx.spawn(async move |this, cx| {
            let written = receiver.await.unwrap_or_else(|_| Written {
                result: Err(SaveError::Failed("Could not save.".to_owned())),
                ends: None,
            });
            let _ = this.update(cx, |view, cx| view.saved(revision, written, cx));
        }));
        cx.notify();
    }

    fn saved(&mut self, revision: u64, written: Written, cx: &mut Context<Self>) {
        self.saving = None;
        match written.result {
            Ok(stamp) => {
                self.stamp = stamp;
                if written.ends.is_some() {
                    self.ends = written.ends;
                }
                self.dirty = self.revision != revision;
                self.note = Some(SAVED.into());
                if let Some(stamp) = stamp.filter(|_| self.persists) {
                    restamp(self, stamp, cx);
                }
            }
            Err(SaveError::Changed) => {
                self.overwrite = true;
                self.note = Some(CHANGED_WARNING.into());
            }
            Err(SaveError::Failed(error)) => self.note = Some(error.into()),
        }
        (self.host.on_saved)(cx);
        cx.notify();
    }
}
