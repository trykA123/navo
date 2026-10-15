use rusty_core::Location;
use rusty_core::encoding::TextEncoding;

use super::EditorView;
use crate::prelude::*;
use crate::window_view::viewer::{FileHooks, FileStamp, Save, SaveError};

const SAVING: &str = "Saving…";
const SAVED: &str = "Saved.";
const CHANGED_WARNING: &str = "Changed on disk since it was opened. Ctrl+S again overwrites it.";

struct SaveJob {
    location: Location,
    text: String,
    crlf: bool,
    encoding: TextEncoding,
    expected: Option<FileStamp>,
}

impl SaveJob {
    fn write(&self, hooks: &FileHooks) -> Result<Option<FileStamp>, SaveError> {
        let save = Save {
            text: &self.text,
            crlf: self.crlf,
            expected: self.expected,
            encoding: self.encoding,
        };
        (hooks.write)(&self.location, &save)
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
            let result = receiver
                .await
                .unwrap_or_else(|_| Err(SaveError::Failed("Could not save.".to_owned())));
            let _ = this.update(cx, |view, cx| view.saved(revision, result, cx));
        }));
        cx.notify();
    }

    fn saved(
        &mut self,
        revision: u64,
        result: Result<Option<FileStamp>, SaveError>,
        cx: &mut Context<Self>,
    ) {
        self.saving = None;
        match result {
            Ok(stamp) => {
                self.stamp = stamp;
                self.dirty = self.revision != revision;
                self.note = Some(SAVED.into());
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
