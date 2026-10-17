#![forbid(unsafe_code)]

mod atomic_write;
mod cache_db;
mod db;
mod dirs;
mod journal;
mod session;
mod settings;

pub use atomic_write::{backup_path, replace_atomic, write_atomic};
pub use cache_db::{CacheDb, CacheError, CacheOpen};
pub use db::{Db, DbError, FileMemory, OpenReport, Stamp};
pub use dirs::StoreDirs;
pub use journal::{
    JobId, JobJournal, JobState, JournalError, JournalStep, StepState, UnfinishedJob,
};
pub use session::{
    SESSION_MAGIC, SESSION_VERSION, SessionFileError, SessionSource, load_session,
    read_session_file, write_session,
};
pub use settings::{
    Appearance, Completion, DefaultChange, Density, Editor, Files, HiddenFiles, LoadReport, Saved,
    SelectionSettings, SettingValue, Settings, SettingsError, SettingsStore, SortKey, Terminal,
    ThemeMode, key_line as settings_key_line, parse as parse_settings, to_toml as settings_to_toml,
};
