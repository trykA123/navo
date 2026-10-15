use std::path::{Path, PathBuf};

use crate::{Language, RulesError};

#[derive(Debug, Clone, thiserror::Error)]
#[error("{}: {error}", path.display())]
pub struct LoadError {
    pub path: PathBuf,
    pub error: RulesError,
}

#[derive(Debug, Clone, Default)]
pub struct Registry {
    languages: Vec<Language>,
}

impl Registry {
    #[must_use]
    pub fn builtin() -> Self {
        Self::default()
    }

    pub fn load_dir(&mut self, _dir: &Path) -> Vec<LoadError> {
        Vec::new()
    }

    #[must_use]
    pub fn for_name(&self, _file_name: &str) -> Option<&Language> {
        None
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Language> {
        self.languages.iter().find(|lang| lang.name == name)
    }
}

#[cfg(test)]
mod tests;
