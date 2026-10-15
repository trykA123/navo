#![forbid(unsafe_code)]

mod atomic_write;
mod settings;

pub use atomic_write::{backup_path, write_atomic};
pub use settings::{
    Appearance, Density, Files, LoadReport, SelectionSettings, Settings, SettingsError,
    SettingsStore, SortKey, Terminal, ThemeMode, parse as parse_settings,
    to_toml as settings_to_toml,
};
