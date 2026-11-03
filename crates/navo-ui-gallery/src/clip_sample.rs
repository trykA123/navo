//! The sample clipboard behind the palette pages: history, saved items, secrets and notes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use navo_core::Location;
use navo_ui::clip_palette::{
    ClipCounts, ClipId, ClipKind, ClipMode, ClipResults, ClipRow, ClipboardHooks, How, Origin,
    SavedDraft, SavedEdit, Scope, SecretText,
};

pub(crate) const NOW: i64 = 1_793_000_000_000;
const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
pub(crate) const SNIPPET_ROW: usize = HISTORY.len() + 1;
pub(crate) const SECRET_ROW: usize = HISTORY.len() + SAVED.len() - 2;
pub(crate) const PROJECT: &str = r"D:\Projects\ecu-a2";
const OUT: &str = r"D:\Projects\ecu-a2\build\out";
const CALIB: &str = r"\\fileserver\calib\r7";
pub(crate) const GONE: &str = r"D:\Projects\old-ecu";

type Sample = (ClipKind, &'static str, &'static str, &'static str, i64);

const HISTORY: [Sample; 10] = [
    (
        ClipKind::Text,
        "Flash with TASKING 6.3r1; 6.2 breaks the .mdf import",
        "",
        "Teams",
        MINUTE,
    ),
    (
        ClipKind::Files,
        "3 files · ecu_a2_main.elf, ecu_a2_main.map, ecu_a2.mdf",
        "D:\\Projects\\ecu-a2\\build\\out\\ecu_a2_main.elf\nD:\\Projects\\ecu-a2\\build\\out\\ecu_a2_main.map\nD:\\Projects\\ecu-a2\\build\\out\\ecu_a2.mdf",
        "Navo",
        4 * MINUTE,
    ),
    (
        ClipKind::Path,
        r"D:\Projects\ecu-a2\build\out\ecu_a2_main.elf",
        "",
        "Navo",
        6 * MINUTE,
    ),
    (
        ClipKind::Text,
        "cargo xtask check -p navo-index",
        "",
        "Terminal",
        12 * MINUTE,
    ),
    (
        ClipKind::Link,
        "https://ci.example.lan/job/ecu-a2/1482/console",
        "",
        "Edge",
        25 * MINUTE,
    ),
    (
        ClipKind::Text,
        "0x8000_4A10  .text.CanIf_RxIndication  1 284 B",
        "",
        "Navo F3",
        40 * MINUTE,
    ),
    (
        ClipKind::Files,
        "2 files · DBC_PT_CAN_v14.dbc, A2L_ecu_a2_r7.a2l",
        "\\\\fileserver\\calib\\r7\\DBC_PT_CAN_v14.dbc\n\\\\fileserver\\calib\\r7\\A2L_ecu_a2_r7.a2l",
        "Navo",
        HOUR,
    ),
    (
        ClipKind::Text,
        "git log --oneline -20 origin/main",
        "",
        "Terminal",
        2 * HOUR,
    ),
    (
        ClipKind::Text,
        "Review notes for the CAN gateway: timeouts at 50 ms, not 20.",
        "",
        "Outlook",
        DAY + 2 * HOUR,
    ),
    (
        ClipKind::Path,
        r"\\fileserver\calib\r7\A2L_ecu_a2_r7.a2l",
        "",
        "Navo",
        DAY + 5 * HOUR,
    ),
];

const SAVED: [Sample; 8] = [
    (
        ClipKind::Text,
        "Check the index crate",
        "cargo xtask check -p navo-index",
        "",
        0,
    ),
    (
        ClipKind::Text,
        "Flash ECU A2 over CAN",
        "tricore-flash --port PCAN_USB1 --baud 500k build\\out\\ecu_a2_main.hex",
        PROJECT,
        2 * DAY,
    ),
    (ClipKind::Path, "ECU A2 build output", OUT, "", 0),
    (
        ClipKind::Path,
        "Calibration share",
        r"\\fileserver\calib\r7",
        "",
        7 * DAY,
    ),
    (
        ClipKind::Text,
        "Review reply",
        "Thanks, merged. The flash script now checks the .mdf version first.",
        "",
        3 * DAY,
    ),
    (ClipKind::Text, "Signature", "Regards,\nadmin", "", 0),
    (ClipKind::Secret, "Build server SSH", "admin", "", 0),
    (ClipKind::Secret, "Artifactory token", "ci-bot", "", 7 * DAY),
];

/// Notes: title, detail, folder (empty for the global note), age and text.
type Note = (&'static str, &'static str, &'static str, i64, &'static str);

const NOTES: [Note; 4] = [
    (
        "ecu-a2",
        "3 lines · 158 characters",
        PROJECT,
        2 * HOUR,
        "Flash with TASKING 6.3r1; 6.2 breaks the .mdf import.\nPower-cycle after flashing, the bootloader keeps the old CRC.\nOpen: confirm the A2L revision with the calibration team.",
    ),
    (
        "Global note",
        "4 lines · 98 characters",
        "",
        3 * DAY,
        "Release checklist\n1. Tag the commit\n2. Build both targets\n3. Upload the zip and the map files",
    ),
    (
        "r7",
        "2 lines · 61 characters",
        CALIB,
        7 * DAY,
        "Calibration share, read only.\nAsk for the r8 folder in March.",
    ),
    (
        "old-ecu",
        "1 line · 33 characters",
        GONE,
        20 * DAY,
        "Archived. Moved to the NAS in May.",
    ),
];

/// What `reveal` gives back, by title; a secret row carries only its user name.
const SECRETS: [(&str, &str); 2] = [
    ("Build server SSH", "k7#Qm2-vX9!pLr4e"),
    ("Artifactory token", "AKCp8ihxR2b9TqWm4sLkZ7"),
];

fn clip_row(id: u64, from: Origin, (kind, title, detail, extra, ago): Sample) -> ClipRow {
    let (source_app, scope) = match from {
        Origin::Clipboard => (Some(extra.to_owned()), Scope::Everywhere),
        Origin::Saved => (
            None,
            Location::new(extra).map_or(Scope::Everywhere, Scope::Folder),
        ),
    };
    ClipRow {
        id: ClipId(id),
        kind,
        title: title.to_owned(),
        detail: detail.to_owned(),
        source_app,
        when_ms: NOW - ago,
        scope,
        from,
        ranges: Vec::new(),
    }
}

pub(crate) struct SampleClipboard {
    rows: Vec<ClipRow>,
    /// The first-open line shows once, as on a first run.
    intro: AtomicBool,
    has_notes: bool,
}

fn scope_of(folder: &str) -> Scope {
    Location::new(folder).map_or(Scope::Everywhere, Scope::Folder)
}

fn note_row(id: u64, (title, detail, folder, ago, _): Note) -> ClipRow {
    ClipRow {
        id: ClipId(id),
        kind: ClipKind::Note,
        title: title.to_owned(),
        detail: detail.to_owned(),
        source_app: None,
        when_ms: NOW - ago,
        scope: scope_of(folder),
        from: Origin::Saved,
        ranges: Vec::new(),
    }
}

impl SampleClipboard {
    pub(crate) fn new(has_intro: bool, has_notes: bool) -> Arc<Self> {
        let history = HISTORY.iter().zip(0..);
        let saved = SAVED.iter().zip(100..);
        let notes = NOTES.iter().zip(300..);
        let rows = history
            .map(|(sample, id)| clip_row(id, Origin::Clipboard, *sample))
            .chain(saved.map(|(sample, id)| clip_row(id, Origin::Saved, *sample)))
            .chain(notes.map(|(note, id)| note_row(id, *note)))
            .collect();
        Arc::new(Self {
            rows,
            intro: AtomicBool::new(has_intro),
            has_notes,
        })
    }

    fn note_text(&self, scope: &Scope) -> Option<String> {
        NOTES
            .iter()
            .find(|note| scope_of(note.2) == *scope)
            .map(|note| note.4.to_owned())
    }
}

fn is_in(mode: ClipMode, row: &ClipRow) -> bool {
    let is_note = row.kind == ClipKind::Note;
    match mode {
        ClipMode::All => !is_note,
        ClipMode::Recent => row.from == Origin::Clipboard,
        ClipMode::Saved => row.from == Origin::Saved && row.kind != ClipKind::Secret && !is_note,
        ClipMode::Secrets => row.kind == ClipKind::Secret,
        ClipMode::Notes => is_note,
    }
}

impl ClipboardHooks for SampleClipboard {
    fn counts(&self) -> Option<ClipCounts> {
        Some(ClipCounts::default())
    }

    fn modes(&self) -> Vec<ClipMode> {
        let modes = [
            ClipMode::All,
            ClipMode::Recent,
            ClipMode::Saved,
            ClipMode::Secrets,
        ];
        modes
            .into_iter()
            .chain(self.has_notes.then_some(ClipMode::Notes))
            .collect()
    }

    fn search(&self, query: &str, mode: ClipMode, _: Option<&Location>) -> ClipResults {
        let query = query.to_lowercase();
        let rows = self
            .rows
            .iter()
            .filter(|row| is_in(mode, row) && row.title.to_lowercase().contains(&query))
            .cloned()
            .collect();
        ClipResults {
            rows,
            kept: HISTORY.len() as u32,
            revision: 1,
            notice: None,
        }
    }

    fn copy(&self, _: ClipId, _: How) -> Result<(), String> {
        Ok(())
    }

    fn delete(&self, _: ClipId) -> Result<(), String> {
        Ok(())
    }

    fn clear_history(&self) -> Result<(), String> {
        Ok(())
    }

    fn delete_all(&self) -> Result<(), String> {
        Ok(())
    }

    fn set_recording(&self, _: bool) -> Result<(), String> {
        Ok(())
    }

    fn subscribe(&self, _: Box<dyn Fn() + Send + Sync>) {}

    fn save_from_history(&self, id: ClipId) -> Result<ClipId, String> {
        Ok(id)
    }

    fn save(&self, _: SavedDraft) -> Result<ClipId, String> {
        Ok(ClipId(0))
    }

    fn saved_for_edit(&self, id: ClipId) -> Result<SavedEdit, String> {
        let row = self
            .rows
            .iter()
            .find(|row| row.id == id && row.from == Origin::Saved);
        let row = row.ok_or_else(|| "not found".to_owned())?;
        let is_secret = row.kind == ClipKind::Secret;
        Ok(SavedEdit {
            kind: row.kind,
            title: row.title.clone(),
            body: SecretText::new(if is_secret {
                String::new()
            } else {
                row.detail.clone()
            }),
            user: is_secret.then(|| row.detail.clone()),
            scope: row.scope.clone(),
        })
    }

    fn update_saved(&self, _: ClipId, _: SavedDraft) -> Result<(), String> {
        Ok(())
    }

    fn rename_note(&self, _: Scope, _: Scope) -> Result<(), String> {
        Ok(())
    }

    fn note_for(&self, folder: &Location) -> Option<String> {
        self.note_text(&Scope::Folder(folder.clone()))
    }

    fn global_note(&self) -> Option<String> {
        self.note_text(&Scope::Everywhere)
    }

    fn set_note(&self, _: Scope, _: String) -> Result<(), String> {
        Ok(())
    }

    fn reveal(&self, id: ClipId) -> Result<SecretText, String> {
        let row = self.rows.iter().find(|row| row.id == id);
        let secret = SECRETS
            .iter()
            .find(|(title, _)| Some(*title) == row.map(|r| r.title.as_str()));
        secret
            .map(|(_, text)| SecretText::new((*text).to_owned()))
            .ok_or_else(String::new)
    }

    fn copy_user(&self, _: ClipId) -> Result<(), String> {
        Ok(())
    }

    fn take_intro(&self) -> bool {
        self.intro.swap(false, Ordering::SeqCst)
    }
}
