use std::sync::Arc;

use navo_clipboard::{ClipService, FailKind, Mode};
use navo_core::Location;
use navo_ui::clip_palette::{
    ClipCounts, ClipId, ClipMode, ClipResults, ClipboardHooks, How, SavedDraft, Scope, SecretText,
};

use super::rows::{self, Target};
use super::running::failed_notice;
use super::saved::{WIPE_WAIT, item_of, message, now_ms, scope_of, wait};
use super::{ClipModule, folder_path};

type Refusal = Result<(), String>;

fn off() -> String {
    navo_l10n::tr("clipboard-off")
}

fn queued(sent: bool) -> Refusal {
    if sent {
        Ok(())
    } else {
        Err(navo_l10n::tr("clipboard-busy"))
    }
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
    match reply.recv_timeout(WIPE_WAIT) {
        Ok(Err(FailKind::Store)) => Err(navo_l10n::tr("clipboard-not-deleted")),
        Ok(Err(kind)) => Err(message(kind)),
        Ok(Ok(())) => Ok(()),
        Err(_) => Err(message(FailKind::Busy)),
    }
}

impl ClipModule {
    /// The running service; a worker that has died reads as a failed recorder.
    fn running(&self) -> Result<Arc<ClipService>, String> {
        let (service, _) = self.peek().live.ok_or_else(off)?;
        if service.is_alive() {
            Ok(service)
        } else {
            Err(failed_notice())
        }
    }
}

impl ClipboardHooks for ClipModule {
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

    /// Notes is left out: the hooks cannot list folder notes yet.
    fn modes(&self) -> Vec<ClipMode> {
        match self.peek().live {
            Some(_) => vec![
                ClipMode::All,
                ClipMode::Recent,
                ClipMode::Saved,
                ClipMode::Secrets,
            ],
            None => Vec::new(),
        }
    }

    fn search(&self, query: &str, mode: ClipMode, folder: Option<&Location>) -> ClipResults {
        let view = self.peek();
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
        }
    }

    fn delete(&self, id: ClipId) -> Refusal {
        let service = self.running()?;
        match rows::target(id).ok_or_else(off)? {
            Target::History(row) => queued(service.delete(row)),
            Target::Saved(row) => queued(service.delete_saved(row)),
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
    /// items; files and images are refused. A secret needs a build that can seal and carries
    /// no user name yet.
    fn save(&self, draft: SavedDraft) -> Result<ClipId, String> {
        let service = self.running()?;
        let (item, secret) = item_of(&draft)?;
        if secret.is_some() && !self.backends.secrets {
            return Err(message(FailKind::SecretsUnsupported));
        }
        let saved = wait(service.save(item, secret))?;
        rows::saved_id(saved).ok_or_else(|| message(FailKind::Store))
    }

    fn note_for(&self, folder: &Location) -> Option<String> {
        let service = self.running().ok()?;
        let scope = navo_clipboard::Scope::Folder(folder_path(Some(folder))?);
        wait(service.note(scope)).ok().flatten()
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
