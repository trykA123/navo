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
    /// A folder note or the global note; its text never reaches a row.
    Note,
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

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// An explicit second wiped buffer, for a save that may fail and be tried again.
    #[must_use]
    pub fn duplicate(&self) -> Self {
        Self(self.0.clone())
    }

    /// Appends text. A full buffer moves to a larger one and the old one is wiped, so no
    /// stale copy stays behind.
    pub fn push_str(&mut self, text: &str) {
        let needed = self.0.len() + text.len();
        if needed > self.0.capacity() {
            let mut grown = Vec::with_capacity(needed.max(64).next_power_of_two());
            grown.extend_from_slice(&self.0);
            wipe(&mut self.0);
            self.0 = grown;
        }
        self.0.extend_from_slice(text.as_bytes());
    }

    /// Appends one character, as `push_str` does.
    pub fn push(&mut self, c: char) {
        self.push_str(c.encode_utf8(&mut [0; 4]));
    }

    /// Removes the last character and wipes its bytes.
    pub fn pop(&mut self) {
        let Some(last) = self.expose().chars().next_back() else {
            return;
        };
        let keep = self.0.len() - last.len_utf8();
        self.0[keep..].fill(0);
        self.0.truncate(keep);
    }

    /// Wipes and empties the buffer.
    pub fn clear(&mut self) {
        wipe(&mut self.0);
        self.0.clear();
    }
}

/// Wipes the bytes of a string that held a secret, then frees it.
pub(super) fn wipe_text(text: String) {
    wipe(&mut text.into_bytes());
}

fn wipe(bytes: &mut [u8]) {
    bytes.iter_mut().for_each(|byte| *byte = 0);
    std::hint::black_box(&*bytes);
}

impl Drop for SecretText {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

impl fmt::Debug for SecretText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretText(..)")
    }
}

/// An entry the user writes by hand. `user` is a secret's user name; empty or `None` for other
/// kinds. `Debug` prints the kind only: title, user name and body are user data.
pub struct SavedDraft {
    pub kind: ClipKind,
    pub title: String,
    pub body: SecretText,
    pub user: Option<String>,
    pub scope: Scope,
}

impl fmt::Debug for SavedDraft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedDraft")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// A saved item loaded for the edit form. `body` is empty for a secret: its password is shown
/// only through `reveal`. It has no `Debug`: every field is user data.
pub struct SavedEdit {
    pub kind: ClipKind,
    pub title: String,
    pub body: SecretText,
    pub user: Option<String>,
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
    /// Loads a saved item for the edit form. Never returns a secret's password.
    fn saved_for_edit(&self, id: ClipId) -> Result<SavedEdit, String>;
    /// Replaces the saved item `id`. A secret whose draft body is empty keeps its password;
    /// a non-empty body replaces it. A secret becomes another kind only with a non-empty body.
    /// Waits a bounded time like `save`.
    fn update_saved(&self, id: ClipId, draft: SavedDraft) -> Result<(), String>;
    /// The text of the folder's note; `None` when there is none or it could not be read.
    fn note_for(&self, folder: &Location) -> Option<String>;
    /// The same for the global note.
    fn global_note(&self) -> Option<String>;
    fn set_note(&self, scope: Scope, text: String) -> Result<(), String>;
    /// Points the note of folder `from` at folder `to`. Refused when `to` has a note.
    fn rename_note(&self, from: Scope, to: Scope) -> Result<(), String>;
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

    fn saved_for_edit(&self, _id: ClipId) -> Result<SavedEdit, String> {
        off()
    }

    fn update_saved(&self, _id: ClipId, _draft: SavedDraft) -> Result<(), String> {
        off()
    }

    fn note_for(&self, _folder: &Location) -> Option<String> {
        None
    }

    fn global_note(&self) -> Option<String> {
        None
    }

    fn set_note(&self, _scope: Scope, _text: String) -> Result<(), String> {
        off()
    }

    fn rename_note(&self, _from: Scope, _to: Scope) -> Result<(), String> {
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
        assert_eq!(hooks.reveal(ClipId(1)).err(), Some(refusal.clone()));
        assert_eq!(hooks.saved_for_edit(ClipId(1)).err(), Some(refusal.clone()));
        assert_eq!(hooks.update_saved(ClipId(1), draft()), Err(refusal.clone()));
        let (from, to) = (Scope::Everywhere, Scope::Everywhere);
        assert_eq!(hooks.rename_note(from, to), Err(refusal));
    }

    fn draft() -> SavedDraft {
        SavedDraft {
            kind: ClipKind::Secret,
            title: "title-canary".into(),
            body: SecretText::new("body-canary".into()),
            user: Some("user-canary".into()),
            scope: Scope::Everywhere,
        }
    }

    #[test]
    fn a_draft_debug_shows_no_user_data() {
        let text = format!("{:?}", draft());
        for canary in ["title-canary", "body-canary", "user-canary"] {
            assert!(!text.contains(canary), "{text}");
        }
    }

    #[test]
    fn secret_text_edits_by_character_and_wipes() {
        let mut secret = SecretText::new(String::new());
        assert!(secret.is_empty());
        secret.push_str("pä");
        secret.push_str(&"x".repeat(200));
        assert_eq!(secret.expose().chars().count(), 202);
        let copy = secret.duplicate();
        secret.clear();
        assert!(secret.is_empty());
        assert_eq!(copy.expose().chars().count(), 202);
        let mut word = SecretText::new("bä".into());
        word.pop();
        assert_eq!(word.expose(), "b");
        word.pop();
        word.pop();
        assert!(word.is_empty());
    }

    #[test]
    fn secret_text_debug_hides_content() {
        let secret = SecretText::new("hunter2".into());
        assert_eq!(secret.expose(), "hunter2");
        assert!(!format!("{secret:?}").contains("hunter2"));
    }
}
