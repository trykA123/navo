#![forbid(unsafe_code)]

mod atomic_write;
mod cache_db;
mod db;
mod dirs;
mod journal;
mod keymap_file;
mod recovery;
mod retention;
mod session;
mod settings;
mod text_layout;

pub use atomic_write::{backup_path, replace_atomic, replace_entry_atomic, write_atomic};
pub use cache_db::{CacheDb, CacheError, CacheOpen};
pub use db::{
    Collection, CollectionItem, CollectionKind, Db, DbError, FileMemory, Frecent, NewCollection,
    OpenReport, RenamePreset, RenamePresets, Stamp,
};
pub use dirs::StoreDirs;
pub use journal::{
    FinishedJob, JobId, JobJournal, JobState, JournalError, JournalStep, StepEvidence, StepState,
    UnfinishedJob,
};
pub use keymap_file::{KeymapEdit, KeymapFile, KeymapFileError, patch_keymap};
pub use recovery::{
    QuarantineReport, RecoveryError, SalvageOutcome, SalvageReport, SalvagedTable, UnrecoveredJob,
    UnrecoveredRetained, acknowledge_quarantine, quarantine_database, salvage_database,
};
pub use retention::{
    RetainedCompletion, RetentionCatalog, RetentionStoreError, RetentionTransaction,
};
pub use session::{
    SESSION_MAGIC, SESSION_VERSION, SessionFileError, SessionSource, load_session,
    read_session_file, write_session,
};
pub use settings::{
    Appearance, Automotive, AutomotiveView, ColorTheme, Completion, CopyMode, CopySettings,
    DefaultChange, Density, DropConfirm, Editor, Files, HiddenFiles, ImageView, ImageViewer,
    LoadReport, MarkdownView, MarkdownViewer, ModifierHints, PreviewView, PreviewViewer,
    RetentionSettings, Saved, Search, SearchIndex, SelectionSettings, SessionBackup, SettingValue,
    Settings, SettingsError, SettingsStore, SizeMode, SortKey, Terminal, TextView, TextViewer,
    ThemeMode, ViewPreset, WindowSettings, WindowView, key_line as settings_key_line,
    parse as parse_settings, parse_retention as parse_retention_settings,
    section_line as settings_section_line, to_toml as settings_to_toml,
};
