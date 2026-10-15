use std::fs;
use std::path::{Path, PathBuf};

use crate::{Language, RulesError, from_toml, from_udl};

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
        Self {
            languages: crate::builtin(),
        }
    }

    pub fn load_dir(&mut self, dir: &Path) -> Vec<LoadError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => return vec![io_error(dir, &error)],
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .collect();
        paths.sort();
        let mut errors = Vec::new();
        for path in paths {
            let parse = match path.extension().and_then(|ext| ext.to_str()) {
                Some(ext) if ext.eq_ignore_ascii_case("toml") => from_toml,
                Some(ext) if ext.eq_ignore_ascii_case("xml") => from_udl,
                _ => continue,
            };
            let loaded = fs::read_to_string(&path)
                .map_err(|error| RulesError::Io {
                    message: error.to_string(),
                })
                .and_then(|text| parse(&text));
            match loaded {
                Ok(languages) => languages.into_iter().for_each(|lang| self.add(lang)),
                Err(error) => errors.push(LoadError { path, error }),
            }
        }
        errors
    }

    pub fn add(&mut self, lang: Language) {
        self.languages.retain(|old| old.name != lang.name);
        self.languages.push(lang);
    }

    #[must_use]
    pub fn for_name(&self, file_name: &str) -> Option<&Language> {
        let (_, ext) = file_name.rsplit_once('.')?;
        let ext = ext.to_lowercase();
        self.languages
            .iter()
            .rev()
            .find(|lang| lang.extensions.contains(&ext))
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Language> {
        self.languages.iter().find(|lang| lang.name == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Language> {
        self.languages.iter()
    }
}

fn io_error(path: &Path, error: &std::io::Error) -> LoadError {
    LoadError {
        path: path.to_path_buf(),
        error: RulesError::Io {
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests;
