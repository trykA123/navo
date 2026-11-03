use std::io::{self, Read as _};
use std::path::Path;
use std::sync::Arc;

use navo_clipboard::{ClipService, FailKind, Mode};
use navo_core::Location;
use navo_ui::clip_palette::{
    ClipCounts, ClipId, ClipMode, ClipResults, ClipboardHooks, How, PopUpPlacement, SavedDraft,
    SavedEdit, Scope, SecretText,
};

use super::rows::{self, Target};
use super::running::failed_notice;
use super::saved::{WIPE_WAIT, deleted, deleted_within, item_of, message, now_ms, scope_of, wait};
use super::{ClipModule, folder_path, lock};

type Refusal = Result<(), String>;
const POPUP_FILE: &str = "clipboard-popup.json";

fn off() -> String {
    navo_l10n::tr("clipboard-off")
}

fn mode_of(mode: ClipMode) -> Mode {
    match mode {
        ClipMode::All => Mode::All,
        ClipMode::Recent => Mode::Recent,
        ClipMode::Saved => Mode::Saved,
        ClipMode::Secrets => Mode::Secrets,
        ClipMode::Notes => Mode::Notes,
    }
}

/// A delete-all answer: a refused wipe reads "not deleted", not "failed".
fn wiped(reply: navo_clipboard::Reply<()>) -> Refusal {
    deleted_within(reply, WIPE_WAIT)
}

impl ClipModule {
    pub(super) fn load_popup(data: Option<&Path>) -> Option<PopUpPlacement> {
        let path = data?.join(POPUP_FILE);
        let read = || -> io::Result<PopUpPlacement> {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(4097)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 4096 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "pop-up state is too large",
                ));
            }
            let saved: PopUpPlacement = serde_json::from_slice(&bytes)?;
            let rect = saved.rect;
            if ![rect.x, rect.y, rect.width, rect.height]
                .iter()
                .all(|n| n.is_finite())
                || rect.width <= 0.
                || rect.height <= 0.
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid pop-up bounds",
                ));
            }
            Ok(saved)
        };
        match read() {
            Ok(saved) => Some(saved),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                tracing::warn!(%error, "loading clipboard pop-up placement failed");
                None
            }
        }
    }

    /// The running service; a worker that has died reads as a failed recorder.
    pub(super) fn running(&self) -> Result<Arc<ClipService>, String> {
        let (service, _) = self.peek().live.ok_or_else(off)?;
        if service.is_alive() {
            Ok(service)
        } else {
            Err(failed_notice())
        }
    }
}

impl ClipboardHooks for ClipModule {
    fn popup_placement(&self) -> Option<PopUpPlacement> {
        *lock(&self.popup)
    }

    fn save_popup_placement(&self, placement: PopUpPlacement) {
        let mut current = lock(&self.popup);
        if *current == Some(placement) {
            return;
        }
        *current = Some(placement);
        drop(current);
        let Some(data) = self
            .db
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_owned)
        else {
            return;
        };
        self.jobs.run("navo-popup-save", move || {
            let save = || -> io::Result<()> {
                std::fs::create_dir_all(&data)?;
                navo_store::write_atomic(&data.join(POPUP_FILE), &serde_json::to_vec(&placement)?)
            };
            if let Err(error) = save() {
                tracing::warn!(%error, "saving clipboard pop-up placement failed");
            }
        });
    }

    fn counts(&self) -> Option<ClipCounts> {
        let count = |len: usize| u32::try_from(len).unwrap_or(u32::MAX);
        Some(match self.peek().live {
            Some((_, snapshot)) => ClipCounts {
                history: count(snapshot.history.len()),
                saved: count(snapshot.saved.len() + snapshot.saved_skipped),
                notes: count(snapshot.notes.len() + snapshot.notes_skipped),
            },
            None => ClipCounts::default(),
        })
    }

    fn modes(&self) -> Vec<ClipMode> {
        match self.peek().live {
            Some(_) => vec![
                ClipMode::All,
                ClipMode::Recent,
                ClipMode::Saved,
                ClipMode::Secrets,
                ClipMode::Notes,
            ],
            None => Vec::new(),
        }
    }

    fn search(&self, query: &str, mode: ClipMode, folder: Option<&Location>) -> ClipResults {
        let view = self.peek();
        if mode == ClipMode::Notes {
            return self.note_results(view, query, folder);
        }
        let Some((_, snapshot)) = view.live else {
            return ClipResults {
                notice: view.notice,
                revision: view.revision,
                ..ClipResults::default()
            };
        };
        let folder = folder_path(folder);
        let found =
            navo_clipboard::search(snapshot.rows(), mode_of(mode), query, folder.as_deref());
        let needs_windows = mode == ClipMode::Secrets && !self.backends.secrets;
        ClipResults {
            rows: found.iter().filter_map(rows::row).collect(),
            kept: u32::try_from(snapshot.history.len()).unwrap_or(u32::MAX),
            revision: view.revision,
            notice: view
                .notice
                .or_else(|| needs_windows.then(|| message(FailKind::SecretsUnsupported))),
        }
    }

    fn copy(&self, id: ClipId, how: How) -> Refusal {
        let service = self.running()?;
        let how = match how {
            How::Copy => navo_clipboard::How::Copy,
            How::Paste => navo_clipboard::How::Paste,
        };
        match rows::target(id).ok_or_else(off)? {
            Target::History(row) => wait(service.copy_answered(row, how)),
            Target::Saved(row) => wait(service.copy_saved(row, how)),
            Target::Note(_) => Err(message(FailKind::Refused)),
        }
    }

    fn delete(&self, id: ClipId) -> Refusal {
        let service = self.running()?;
        match rows::target(id).ok_or_else(off)? {
            Target::History(row) => deleted(service.delete(row)),
            Target::Saved(row) => deleted(service.delete_saved(row)),
            Target::Note(hash) => self.delete_note(&service, hash),
        }
    }

    fn clear_history(&self) -> Refusal {
        wait(self.running()?.clear_history())
    }

    fn delete_all(&self) -> Refusal {
        match self.running() {
            Ok(service) => wiped(service.wipe_all()),
            Err(_) if self.peek().live.is_none() => wiped(self.wipe_while_off()),
            Err(failed) => Err(failed),
        }
    }

    fn set_recording(&self, on: bool) -> Refusal {
        self.running()?.set_recording(on);
        Ok(())
    }

    fn subscribe(&self, on_change: Box<dyn Fn() + Send + Sync>) {
        self.listeners.add(Arc::from(on_change));
    }

    fn save_from_history(&self, id: ClipId) -> Result<ClipId, String> {
        let service = self.running()?;
        let Some(Target::History(row)) = rows::target(id) else {
            return Err(message(FailKind::Refused));
        };
        let saved = wait(service.save_from_history(row, now_ms()))?;
        rows::saved_id(saved).ok_or_else(|| message(FailKind::Store))
    }

    /// The draft's kind picks the saved kind: text and links save as snippets, paths as path
    /// items; files and images are refused. A secret needs a build that can seal; its user
    /// name is kept, other kinds drop it.
    fn save(&self, draft: SavedDraft) -> Result<ClipId, String> {
        let service = self.running()?;
        let (item, secret) = item_of(&draft)?;
        if secret.is_some() && !self.backends.secrets {
            return Err(message(FailKind::SecretsUnsupported));
        }
        let saved = wait(service.save(item, secret))?;
        rows::saved_id(saved).ok_or_else(|| message(FailKind::Store))
    }

    fn saved_for_edit(&self, id: ClipId) -> Result<SavedEdit, String> {
        self.load_for_edit(id)
    }

    fn update_saved(&self, id: ClipId, draft: SavedDraft) -> Refusal {
        self.update_from(id, &draft)
    }

    fn rename_note(&self, from: Scope, to: Scope) -> Refusal {
        self.move_note_to(&from, &to)
    }

    fn note_for(&self, folder: &Location) -> Option<String> {
        let service = self.running().ok()?;
        let scope = navo_clipboard::Scope::Folder(folder_path(Some(folder))?);
        wait(service.note(scope)).ok().flatten()
    }

    fn global_note(&self) -> Option<String> {
        let service = self.running().ok()?;
        wait(service.note(navo_clipboard::Scope::Global))
            .ok()
            .flatten()
    }

    fn set_note(&self, scope: Scope, text: String) -> Refusal {
        let service = self.running()?;
        wait(service.set_note(scope_of(&scope)?, text, now_ms()))
    }

    fn reveal(&self, id: ClipId) -> Result<SecretText, String> {
        let service = self.running()?;
        let Some(Target::Saved(row)) = rows::target(id) else {
            return Err(message(FailKind::Refused));
        };
        let secret = wait(service.reveal(row))?;
        let text = secret.as_str().ok_or_else(|| message(FailKind::Seal))?;
        Ok(SecretText::new(text.to_owned()))
    }

    fn take_intro(&self) -> bool {
        self.peek().live.is_some() && self.intro.take()
    }

    fn copy_user(&self, id: ClipId) -> Refusal {
        let service = self.running()?;
        let Some(Target::Saved(row)) = rows::target(id) else {
            return Err(message(FailKind::Refused));
        };
        wait(service.copy_user(row)).map(drop)
    }
}

#[cfg(test)]
mod popup_tests {
    use super::*;
    use crate::clip_module::{Backends, tests::Rig};
    use navo_ui::clip_palette::{PopUpPlacement, PopUpRect};

    fn placement() -> PopUpPlacement {
        PopUpPlacement {
            display: Some([1; 16]),
            rect: PopUpRect {
                x: -1500.,
                y: 90.,
                width: 1100.,
                height: 650.,
            },
        }
    }

    #[test]
    fn popup_placement_round_trips_through_the_profile_store() {
        let rig = Rig::new();
        assert_eq!(rig.module.popup_placement(), None);
        rig.module.save_popup_placement(placement());
        rig.module.stop();
        let restarted = ClipModule::new(Some(rig.data.clone()), Backends::shell());
        assert_eq!(restarted.popup_placement(), Some(placement()));
    }

    #[test]
    fn an_invalid_popup_record_uses_the_default_placement() {
        let rig = Rig::new();
        rig.module.save_popup_placement(placement());
        rig.module.stop();
        let mut invalid = serde_json::to_value(placement()).unwrap();
        invalid["rect"]["width"] = serde_json::json!(-1);
        std::fs::write(
            rig.data.join(POPUP_FILE),
            serde_json::to_vec(&invalid).unwrap(),
        )
        .unwrap();
        let restarted = ClipModule::new(Some(rig.data.clone()), Backends::shell());
        assert_eq!(restarted.popup_placement(), None);
    }
}
