use std::io;
use std::path::{Path, PathBuf};

use navo_core::brand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDirs {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl StoreDirs {
    #[must_use]
    pub fn detect() -> Option<Self> {
        match std::env::current_exe() {
            Ok(exe) => Self::detect_at(&exe),
            Err(_) => Self::resolve(cfg!(windows), environment),
        }
    }

    /// Resolves the folders as if Navo ran from `exe`, reading the real environment; the portable
    /// audit test (NV9b) uses it to start from a fake portable folder.
    #[must_use]
    pub fn detect_at(exe: &Path) -> Option<Self> {
        Self::detect_for(exe, cfg!(windows), environment)
    }

    fn detect_for(exe: &Path, windows: bool, var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        if let Some(root) = portable_root(exe) {
            return Some(Self {
                config: root.clone(),
                data: root,
            });
        }
        Self::resolve(windows, var)
    }

    /// Returns `<root>/data/run` when this exe runs portable, else `None`. Instance and crash
    /// runtime files go there, so a portable copy writes nothing to the temp folder (NV9a).
    #[must_use]
    pub fn portable_run() -> Option<PathBuf> {
        Self::portable_run_at(&std::env::current_exe().ok()?)
    }

    /// The portable runtime folder for `exe`; `None` when `exe` is not a portable copy.
    #[must_use]
    pub fn portable_run_at(exe: &Path) -> Option<PathBuf> {
        Some(portable_root(exe)?.join("data").join("run"))
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
        Self::legacy(cfg!(windows), environment)
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
    pub fn user_commands_file(&self) -> PathBuf {
        self.config.join("user-commands.toml")
    }

    #[must_use]
    pub fn session_file(&self) -> PathBuf {
        self.data.join("session.bin")
    }

    pub fn ensure(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.config)?;
        std::fs::create_dir_all(&self.data)
    }

    /// Removes temp files that interrupted atomic writes left in the config and data folders.
    pub fn remove_orphan_temps(&self) -> io::Result<usize> {
        Ok(crate::remove_orphan_temps(&self.config)? + crate::remove_orphan_temps(&self.data)?)
    }
}

fn environment(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn portable_root(exe: &Path) -> Option<PathBuf> {
    let folder = exe.parent()?;
    if let Some(root) = velopack_root(exe) {
        return (portable_marker(folder) && root.join(".portable").exists()).then_some(root);
    }
    portable_marker(folder).then_some(folder.to_path_buf())
}

fn velopack_root(exe: &Path) -> Option<PathBuf> {
    let folder = exe.parent()?;
    let root = if folder
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("current"))
    {
        folder.parent()?
    } else {
        folder
    };
    root.join("Update.exe")
        .exists()
        .then_some(root.to_path_buf())
}

fn portable_marker(folder: &Path) -> bool {
    folder.join(format!("{}.toml", brand::SLUG)).is_file()
}

#[cfg(test)]
mod tests;
