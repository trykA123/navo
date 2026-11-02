use std::fmt;
use std::ops::Range;

use navo_core::Location;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClipId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    All,
    Recent,
    Saved,
    Secrets,
    Notes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    Text,
    Link,
    Path,
    Files,
    Image,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Copy,
    Paste,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Clipboard,
    Saved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Everywhere,
    Folder(Location),
}

/// Text that zeroes its buffer on drop. `Debug` prints nothing of the content.
pub struct SecretText(Vec<u8>);

impl SecretText {
    #[must_use]
    pub fn new(text: String) -> Self {
        Self(text.into_bytes())
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        self.0.iter_mut().for_each(|byte| *byte = 0);
        std::hint::black_box(&self.0);
    }
}

impl fmt::Debug for SecretText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretText(..)")
    }
}

/// An entry the user writes by hand. `Debug` hides the body.
#[derive(Debug)]
pub struct SavedDraft {
    pub kind: ClipKind,
    pub title: String,
    pub body: SecretText,
    pub scope: Scope,
}

/// One palette row. It has no `Debug`: title and detail hold clipboard content.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipRow {
    pub id: ClipId,
    pub kind: ClipKind,
    pub title: String,
    pub detail: String,
    pub source_app: Option<String>,
    pub when_ms: i64,
    pub scope: Scope,
    pub from: Origin,
    pub ranges: Vec<Range<usize>>,
}

/// One search answer. A later `revision` replaces an earlier one.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ClipResults {
    pub rows: Vec<ClipRow>,
    pub kept: u32,
    pub revision: u64,
    pub notice: Option<String>,
}

/// How much the store holds, for the confirm dialogs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClipCounts {
    pub history: u32,
    pub saved: u32,
    pub notes: u32,
}

pub trait ClipboardHooks: Send + Sync {
    /// The store's counts; `None` when this build has no clipboard history.
    fn counts(&self) -> Option<ClipCounts>;
    fn modes(&self) -> Vec<ClipMode>;
    fn search(&self, query: &str, mode: ClipMode, folder: Option<&Location>) -> ClipResults;
    /// Returns once the clipboard holds the item, so a paste may follow at once; an error
    /// means it may not. Waits a bounded time: a slow writer reads as busy.
    fn copy(&self, id: ClipId, how: How) -> Result<(), String>;
    fn delete(&self, id: ClipId) -> Result<(), String>;
    fn clear_history(&self) -> Result<(), String>;
    fn delete_all(&self) -> Result<(), String>;
    fn set_recording(&self, on: bool) -> Result<(), String>;
    fn subscribe(&self, on_change: Box<dyn Fn() + Send + Sync>);
    fn save_from_history(&self, id: ClipId) -> Result<ClipId, String>;
    fn save(&self, draft: SavedDraft) -> Result<ClipId, String>;
    fn note_for(&self, folder: &Location) -> Option<String>;
    fn set_note(&self, scope: Scope, text: String) -> Result<(), String>;
    fn reveal(&self, id: ClipId) -> Result<SecretText, String>;
    fn copy_user(&self, id: ClipId) -> Result<(), String>;
    /// True once per profile, for the first palette that opens with the module on: the palette
    /// shows the "kept on this PC" line. The answer is remembered, so the next call is false.
    fn take_intro(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoClipboard;

fn off<T>() -> Result<T, String> {
    Err(navo_l10n::tr("clipboard-off"))
}

impl ClipboardHooks for NoClipboard {
    fn counts(&self) -> Option<ClipCounts> {
        None
    }

    fn modes(&self) -> Vec<ClipMode> {
        Vec::new()
    }

    fn search(&self, _query: &str, _mode: ClipMode, _folder: Option<&Location>) -> ClipResults {
        ClipResults::default()
    }

    fn copy(&self, _id: ClipId, _how: How) -> Result<(), String> {
        off()
    }

    fn delete(&self, _id: ClipId) -> Result<(), String> {
        off()
    }

    fn clear_history(&self) -> Result<(), String> {
        off()
    }

    fn delete_all(&self) -> Result<(), String> {
        off()
    }

    fn set_recording(&self, _on: bool) -> Result<(), String> {
        off()
    }

    fn subscribe(&self, _on_change: Box<dyn Fn() + Send + Sync>) {}

    fn save_from_history(&self, _id: ClipId) -> Result<ClipId, String> {
        off()
    }

    fn save(&self, _draft: SavedDraft) -> Result<ClipId, String> {
        off()
    }

    fn note_for(&self, _folder: &Location) -> Option<String> {
        None
    }

    fn set_note(&self, _scope: Scope, _text: String) -> Result<(), String> {
        off()
    }

    fn reveal(&self, _id: ClipId) -> Result<SecretText, String> {
        off()
    }

    fn copy_user(&self, _id: ClipId) -> Result<(), String> {
        off()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_clipboard_has_no_modes_and_refuses() {
        let hooks = NoClipboard;
        let refusal = navo_l10n::tr("clipboard-off");
        assert!(hooks.modes().is_empty());
        assert!(hooks.search("a", ClipMode::All, None).rows.is_empty());
        assert_eq!(hooks.copy(ClipId(1), How::Copy), Err(refusal.clone()));
        assert_eq!(hooks.delete_all(), Err(refusal.clone()));
        assert_eq!(hooks.reveal(ClipId(1)).err(), Some(refusal));
    }

    #[test]
    fn secret_text_debug_hides_content() {
        let secret = SecretText::new("hunter2".into());
        assert_eq!(secret.expose(), "hunter2");
        assert!(!format!("{secret:?}").contains("hunter2"));
    }
}
