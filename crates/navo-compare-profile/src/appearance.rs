//! Appearance and View preferences: the `[ui]` table of `settings.toml`, read
//! and written without disturbing the tables around it.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::Profile;

/// The table of `settings.toml` that holds the preferences.
const UI_TABLE: &str = "ui";

/// The five palettes of Sidewise (NC66).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaletteId {
    /// The house palette: a violet accent on cool grey.
    #[default]
    Violet,
    /// A neutral grey palette.
    Graphite,
    /// A blue palette.
    Ocean,
    /// A warm orange palette.
    Ember,
    /// High contrast, with thicker focus rings.
    Contrast,
}

/// Which light or dark tokens a palette is projected with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppearanceMode {
    /// Follow the operating system appearance.
    #[default]
    System,
    /// Always the light tokens.
    Light,
    /// Always the dark tokens.
    Dark,
}

/// The View presets of the View menu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewPresetPref {
    /// Unified layout without words, syntax or folding.
    Simple,
    /// Split layout with words, syntax and folding.
    #[default]
    Rich,
    /// The toggles below, chosen one by one.
    Custom,
}

/// The kind of view a pair opens with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewModePref {
    /// Whatever suits the pair, stepping down to bytes when it cannot show.
    #[default]
    Best,
    /// Always text.
    Text,
    /// Always bytes.
    Bytes,
}

/// How a text pair is laid out: two columns, or one merged column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutPref {
    /// Left and right side by side.
    #[default]
    Split,
    /// One column with removed and added lines interleaved.
    Unified,
}

/// The update check preference of a profile (NC180).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateCheck {
    /// Not chosen yet: never checks; the Start screen offers the choice (the default).
    #[default]
    Ask,
    /// Check GitHub at most once a day.
    On,
    /// Never check GitHub.
    Off,
}

/// The `[ui]` table: theme, sidebar and the View options of the View menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    /// Palette the theme tokens come from.
    pub palette: PaletteId,
    /// Light, dark or the operating system.
    pub mode: AppearanceMode,
    /// Whether the session sidebar is collapsed to a rail.
    pub sidebar_collapsed: bool,
    /// Interface scale in percent.
    pub ui_scale_pct: u32,
    /// Active View preset.
    pub preset: ViewPresetPref,
    /// Kind of view a pair opens with.
    pub view_mode: ViewModePref,
    /// Split or unified text layout.
    pub layout: LayoutPref,
    /// Show changed words inside a line.
    pub words: bool,
    /// Show syntax colours.
    pub syntax: bool,
    /// Show line numbers.
    pub line_no: bool,
    /// Fold unchanged lines.
    pub fold_unchanged: bool,
    /// Wrap long lines.
    pub wrap_lines: bool,
    /// Ignore whitespace when comparing.
    pub ignore_whitespace: bool,
    /// Whether Sidewise may check GitHub for a new version (NC180).
    pub update_check: UpdateCheck,
}

impl Default for UiPrefs {
    /// The preferences of a fresh profile: violet, rich, best view.
    fn default() -> Self {
        UiPrefs {
            palette: PaletteId::Violet,
            mode: AppearanceMode::System,
            sidebar_collapsed: false,
            ui_scale_pct: 100,
            preset: ViewPresetPref::Rich,
            view_mode: ViewModePref::Best,
            layout: LayoutPref::Split,
            words: true,
            syntax: true,
            line_no: true,
            fold_unchanged: true,
            wrap_lines: false,
            ignore_whitespace: false,
            update_check: UpdateCheck::Ask,
        }
    }
}

/// Why preferences could not be read or written.
#[derive(Debug)]
pub enum AppearanceError {
    /// A file operation failed at the given path.
    Io {
        /// Path of the failed operation.
        path: PathBuf,
        /// Error returned by the filesystem.
        error: std::io::Error,
    },
}

/// Reads the preferences from `settings.toml`, or the defaults.
///
/// A missing file, a missing `[ui]` table and a table with only some keys give
/// the defaults for what is absent. A file that does not parse, or a `[ui]`
/// table that does not fit, is moved to `settings.toml.bak` and gives the
/// defaults. Fails with `Io` when the file cannot be read or moved.
pub fn load_or_default(profile: &Profile) -> Result<UiPrefs, AppearanceError> {
    let path = profile.settings_file();
    let table = read_table(&path)?;
    let Some(ui) = table.get(UI_TABLE) else {
        return Ok(UiPrefs::default());
    };
    match ui.clone().try_into() {
        Ok(prefs) => Ok(prefs),
        Err(_) => {
            broken_table(&path)?;
            Ok(UiPrefs::default())
        }
    }
}

/// Writes `prefs` as the `[ui]` table of `settings.toml`.
///
/// Every other table and top-level key in the file is kept. A file that does
/// not parse is moved to `settings.toml.bak` first. The new document goes
/// through a temporary file that is synced before it is renamed into place.
/// Fails with `Io` when reading, writing or renaming fails.
pub fn save(profile: &Profile, prefs: &UiPrefs) -> Result<(), AppearanceError> {
    let path = profile.settings_file();
    let mut table = read_table(&path)?;
    let ui =
        toml::Value::try_from(prefs).map_err(|error| io(&path, std::io::Error::other(error)))?;
    table.insert(UI_TABLE.to_string(), ui);
    let text = toml::to_string(&table).map_err(|error| io(&path, std::io::Error::other(error)))?;
    write_atomic(&path, text.as_bytes())
}

/// Reads the settings document, moving a file this build cannot read aside.
///
/// A missing file gives an empty document. Bytes that are not UTF-8 TOML go to
/// `settings.toml.bak` and give an empty document; other read errors give `Io`.
fn read_table(path: &Path) -> Result<toml::Table, AppearanceError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(toml::Table::new()),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return broken_table(path),
        Err(error) => return Err(io(path, error)),
    };
    match toml::from_str(&text) {
        Ok(table) => Ok(table),
        Err(_) => broken_table(path),
    }
}

/// Moves an unreadable settings file to `settings.toml.bak`, keeping its bytes.
fn broken_table(path: &Path) -> Result<toml::Table, AppearanceError> {
    let backup = path.with_extension("toml.bak");
    fs::rename(path, &backup).map_err(|error| io(&backup, error))?;
    Ok(toml::Table::new())
}

/// Writes `bytes` through a temporary file, syncs it and renames it into place.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppearanceError> {
    let temp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    let mut file = File::create(&temp).map_err(|error| io(&temp, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io(&temp, error))?;
    drop(file);
    fs::rename(&temp, path).map_err(|error| io(path, error))
}

/// Wraps an I/O error with the path it happened on.
fn io(path: &Path, error: std::io::Error) -> AppearanceError {
    AppearanceError::Io {
        path: path.to_path_buf(),
        error,
    }
}

#[cfg(test)]
#[path = "appearance_update_tests.rs"]
mod update_tests;
