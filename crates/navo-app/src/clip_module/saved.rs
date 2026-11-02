//! Saved items, secrets and notes behind the hooks. Each call waits a short time for the
//! worker, because the contract answers at once; a slow worker gives the busy message.

use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navo_clipboard::{FailKind, Reply, Saved, SavedKind, SecretBytes};
use navo_ui::clip_palette::{ClipKind, SavedDraft, Scope};

use super::folder_path;

const REPLY_WAIT: Duration = Duration::from_secs(2);
/// A wipe may remove many files and checkpoint the log, so it gets longer.
pub(super) const WIPE_WAIT: Duration = Duration::from_secs(5);

pub(super) fn message(kind: FailKind) -> String {
    let id = match kind {
        FailKind::Busy => "clipboard-busy",
        FailKind::SecretsUnsupported => "clipboard-secrets-need-windows",
        FailKind::Refused => "clipboard-refused",
        FailKind::TooLarge => "clipboard-too-large",
        FailKind::Full => "clipboard-saved-full",
        FailKind::NotFound => "clipboard-not-found",
        FailKind::Store
        | FailKind::Sink
        | FailKind::Thread
        | FailKind::Seal
        | FailKind::NoteExists => "clipboard-failed",
    };
    navo_l10n::tr(id)
}

/// Waits for the worker's answer; a stopped worker or a time-out reads as busy.
pub(super) fn wait<T>(reply: Reply<T>) -> Result<T, String> {
    match reply.recv_timeout(REPLY_WAIT) {
        Ok(result) => result.map_err(message),
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
            Err(message(FailKind::Busy))
        }
    }
}

pub(super) fn now_ms() -> i64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
}

pub(super) fn scope_of(scope: &Scope) -> Result<navo_clipboard::Scope, String> {
    match scope {
        Scope::Everywhere => Ok(navo_clipboard::Scope::Global),
        Scope::Folder(folder) => folder_path(Some(folder))
            .map(navo_clipboard::Scope::Folder)
            .ok_or_else(|| message(FailKind::Refused)),
    }
}

/// The saved item for a draft and, for a secret, the text to seal. The draft's text moves into
/// the secret buffer, which wipes itself on drop.
pub(super) fn item_of(draft: &SavedDraft) -> Result<(Saved, Option<SecretBytes>), String> {
    let kind = match draft.kind {
        ClipKind::Text | ClipKind::Link => SavedKind::Snippet,
        ClipKind::Path => SavedKind::Path,
        ClipKind::Secret => SavedKind::Secret,
        ClipKind::Files | ClipKind::Image => return Err(message(FailKind::Refused)),
    };
    let title = draft.title.trim();
    if title.is_empty() {
        return Err(message(FailKind::Refused));
    }
    let text = draft.body.expose().to_owned();
    let (body, secret) = match kind {
        SavedKind::Secret => (None, Some(SecretBytes::from(text))),
        _ => (Some(text), None),
    };
    let now = now_ms();
    let item = Saved {
        id: 0,
        kind,
        title: title.to_owned(),
        body,
        user: None,
        sealed: None,
        scope: scope_of(&draft.scope)?,
        created_ms: now,
        updated_ms: now,
    };
    Ok((item, secret))
}
