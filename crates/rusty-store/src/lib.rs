#![forbid(unsafe_code)]

mod atomic_write;
mod cache_db;
mod db;
mod dirs;
mod journal;
mod session;
mod settings;

pub use atomic_write::{backup_path, write_atomic};
pub use cache_db::{CacheDb, CacheError, CacheOpen};
pub use db::{Db, DbError, OpenReport};
pub use dirs::StoreDirs;
pub use journal::{
    JobId, JobJournal, JobState, JournalError, JournalStep, StepState, UnfinishedJob,
};
pub use session::{
    SESSION_MAGIC, SESSION_VERSION, SessionFileError, SessionSource, load_session,
    read_session_file, write_session,
};
pub use settings::{
    Appearance, Density, Files, LoadReport, SelectionSettings, Settings, SettingsError,
    SettingsStore, SortKey, Terminal, ThemeMode, parse as parse_settings,
    to_toml as settings_to_toml,
};
