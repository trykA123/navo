use std::io;
use std::path::{Path, PathBuf};

use rusty_core::brand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDirs {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl StoreDirs {
    #[must_use]
    pub fn detect() -> Option<Self> {
        Self::resolve(cfg!(windows), |name| std::env::var(name).ok())
    }

    #[must_use]
    pub fn in_profile(root: &Path) -> Self {
        Self {
            config: root.join("config"),
            data: root.join("data"),
        }
    }

    #[must_use]
    pub fn detect_legacy() -> Option<Self> {
        Self::legacy(cfg!(windows), |name| std::env::var(name).ok())
    }

    pub fn resolve(windows: bool, var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        Self::resolve_named(windows, var, brand::NAME, brand::SLUG)
    }

    pub fn legacy(windows: bool, var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        Self::resolve_named(windows, var, brand::LEGACY_NAME, brand::LEGACY_SLUG)
    }

    fn resolve_named(
        windows: bool,
        var: impl Fn(&str) -> Option<String>,
        name: &str,
        slug: &str,
    ) -> Option<Self> {
        let get = |name: &str| {
            var(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        if windows {
            return Some(Self {
                config: get("APPDATA")?.join(name),
                data: get("LOCALAPPDATA")?.join(name),
            });
        }
        let home = get("HOME");
        let config = get("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config")))?;
        let data = get("XDG_DATA_HOME").or_else(|| home.map(|h| h.join(".local/share")))?;
        Some(Self {
            config: config.join(slug),
            data: data.join(slug),
        })
    }

    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join(format!("{}.toml", brand::SLUG))
    }

    #[must_use]
    pub fn legacy_settings_file(&self) -> PathBuf {
        self.config.join(format!("{}.toml", brand::LEGACY_SLUG))
    }

    #[must_use]
    pub fn journal_file(&self) -> PathBuf {
        Self::journal_in(&self.data)
    }

    #[must_use]
    pub fn journal_in(data: &Path) -> PathBuf {
        data.join(format!("{}.db", brand::SLUG))
    }

    #[must_use]
    pub fn legacy_journal_file(&self) -> PathBuf {
        self.data.join(format!("{}.db", brand::LEGACY_SLUG))
    }

    #[must_use]
    pub fn lock_file(&self) -> PathBuf {
        Self::lock_in(&self.data)
    }

    #[must_use]
    pub fn lock_in(data: &Path) -> PathBuf {
        data.join(format!("{}.lock", brand::SLUG))
    }

    #[must_use]
    pub fn legacy_lock_file(&self) -> PathBuf {
        self.data.join(format!("{}.lock", brand::LEGACY_SLUG))
    }

    #[must_use]
    pub fn keymap_file(&self) -> PathBuf {
        self.config.join("keymap.toml")
    }

    #[must_use]
    pub fn session_file(&self) -> PathBuf {
        self.data.join("session.bin")
    }

    pub fn ensure(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.config)?;
        std::fs::create_dir_all(&self.data)
    }
}

#[cfg(test)]
mod tests;
