use std::io;
use std::path::{Path, PathBuf};

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

    pub fn resolve(windows: bool, var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let get = |name: &str| {
            var(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        if windows {
            return Some(Self {
                config: get("APPDATA")?.join("Ferrite"),
                data: get("LOCALAPPDATA")?.join("Ferrite"),
            });
        }
        let home = get("HOME");
        let config = get("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config")))?;
        let data = get("XDG_DATA_HOME").or_else(|| home.map(|h| h.join(".local/share")))?;
        Some(Self {
            config: config.join("ferrite"),
            data: data.join("ferrite"),
        })
    }

    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("ferrite.toml")
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
