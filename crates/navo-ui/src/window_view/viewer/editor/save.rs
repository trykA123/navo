//! Ctrl+S in the F3 editor: writes the text in the background through the file hooks, in the
//! loaded encoding and line endings or in the conversion chosen in the encoding picker.

use navo_core::Location;
use std::sync::Arc;

use navo_core::encoding::{EncodeError, TextEncoding};
use navo_core::line_ends::LineEnds;

use super::EditorView;
use crate::prelude::*;
use crate::window_view::viewer::search::memory::restamp;
use crate::window_view::viewer::{
    FileHooks, FileStamp, LineEnding, Save, SaveAs, SaveError, Saved,
};

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

impl SaveJob {
    fn write(&self, hooks: &FileHooks) -> Result<Saved, SaveError> {
        let save = Save {
            text: &self.text,
            crlf: self.crlf,
            expected: self.expected,
            encoding: self.encoding,
            ends: self.ends.as_deref(),
        };
        (hooks.write)(&self.location, &save)
    }
}

impl EditorView {
    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        if !self.editing || self.saving.is_some() {
            return;
        }
        let plan = self.save_as;
        let (crlf, ends) = plan.endings_for(self.crlf, self.ends.clone());
        let job = SaveJob {
            location: self.host.location.clone(),
            text: self.editor.read(cx).value().to_string(),
            crlf,
            encoding: plan.encoding_for(self.encoding),
            ends,
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
            let written = receiver
                .await
                .unwrap_or_else(|_| Err(SaveError::Failed("Could not save.".to_owned())));
            let _ = this.update(cx, |view, cx| view.saved((revision, plan), written, cx));
        }));
        cx.notify();
    }

    /// Applies the result of the save started at `revision` with conversion `plan`.
    fn saved(
        &mut self,
        (revision, plan): (u64, SaveAs),
        written: Result<Saved, SaveError>,
        cx: &mut Context<Self>,
    ) {
        self.saving = None;
        match written {
            Ok(Saved { stamp, ends }) => {
                self.stamp = stamp;
                if plan.is_conversion() {
                    self.adopt(plan);
                }
                if let Some(ends) = ends {
                    self.ends = Some(Arc::new(ends));
                }
                self.dirty = self.revision != revision;
                self.edited = self.dirty;
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
            Err(SaveError::Encode(EncodeError::Unmappable { line, character })) => {
                let character = character.to_string();
                self.note = Some(
                    navo_l10n::t!("viewer-save-unmappable", character = character, line = line)
                        .into(),
                );
            }
        }
        (self.host.on_saved)(cx);
        cx.notify();
    }

    /// Makes a written conversion the loaded state, so the next save keeps it. Marks are not
    /// re-saved: their offsets were counted in the old bytes.
    fn adopt(&mut self, plan: SaveAs) {
        self.encoding = plan.encoding_for(self.encoding);
        (self.crlf, self.ends) = plan.endings_for(self.crlf, self.ends.take());
        self.persists = false;
        if self.save_as == plan {
            self.save_as = SaveAs::default();
        }
    }

    /// Sets the conversion the next Ctrl+S writes; it counts as an unsaved change until then.
    /// Going back to the loaded format leaves only the text edits unsaved.
    pub(super) fn set_save_as(&mut self, plan: SaveAs, cx: &mut Context<Self>) {
        let plan = plan.relative_to(self.encoding.encoding);
        if !self.editing || self.save_as == plan {
            return;
        }
        self.save_as = plan;
        self.revision += 1;
        if !plan.is_conversion() {
            self.dirty = self.edited;
            self.note = None;
            cx.notify();
            return;
        }
        self.dirty = true;
        let endings = match plan.line_ending {
            LineEnding::AsLoaded => "loaded",
            LineEnding::Lf => "lf",
            LineEnding::Crlf => "crlf",
        };
        let name = plan.encoding_for(self.encoding).label();
        self.note =
            Some(navo_l10n::t!("viewer-save-as-pending", name = name, endings = endings).into());
        cx.notify();
    }
}
