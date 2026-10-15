use rusty_core::{Command, Location};

use super::{FileHooks, FileStamp, Save, SaveError};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const SAVING: &str = "Saving…";
const SAVED: &str = "Saved.";
const CHANGED_WARNING: &str = "Changed on disk since it was opened. Ctrl+S again overwrites it.";

struct SaveJob {
    location: Location,
    text: String,
    crlf: bool,
    expected: Option<FileStamp>,
}

impl SaveJob {
    fn write(&self, files: &FileHooks) -> Result<Option<FileStamp>, SaveError> {
        let save = Save {
            text: &self.text,
            crlf: self.crlf,
            expected: self.expected,
        };
        (files.write)(&self.location, &save)
    }
}

impl FerriteWindow {
    pub(super) fn save_viewer(&mut self, cx: &mut Context<Self>) {
        let files = self.files.clone();
        let Some(viewer) = self
            .viewer
            .as_mut()
            .filter(|viewer| viewer.editing && viewer.saving.is_none())
        else {
            return;
        };
        let Some(editor) = viewer.editor.clone() else {
            return;
        };
        let job = SaveJob {
            location: viewer.location.clone(),
            text: editor.read(cx).value().to_string(),
            crlf: viewer.crlf,
            expected: if viewer.overwrite { None } else { viewer.stamp },
        };
        let revision = viewer.revision;
        viewer.overwrite = false;
        viewer.message = Some(SAVING.into());
        let (sender, receiver) = futures::channel::oneshot::channel();
        cx.background_spawn(async move {
            let _ = sender.send(job.write(&files));
        })
        .detach();
        viewer.saving = Some(cx.spawn(async move |this, cx| {
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
        if let Some(viewer) = &mut self.viewer {
            viewer.saving = None;
            match result {
                Ok(stamp) => {
                    viewer.stamp = stamp;
                    viewer.dirty = viewer.revision != revision;
                    viewer.message = Some(SAVED.into());
                }
                Err(SaveError::Changed) => {
                    viewer.overwrite = true;
                    viewer.message = Some(CHANGED_WARNING.into());
                }
                Err(SaveError::Failed(error)) => viewer.message = Some(error.into()),
            }
        }
        let _ = self.browser.command(&Command::PaneRefresh);
        cx.notify();
    }
}
