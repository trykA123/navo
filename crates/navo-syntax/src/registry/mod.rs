use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{Language, RulesError, from_toml, from_udl};

/// Largest rule file, in bytes, the loader reads; bigger files are skipped with an error.
const MAX_RULE_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Shape of the two rule-file parsers, `from_toml` and `from_udl`.
type Parse = fn(&str) -> Result<Vec<Language>, RulesError>;

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

    /// Loads `*.toml` and `*.xml` rule files from `dir`, in sorted name order.
    /// Returns one error per unreadable entry or failed file; later files win.
    pub fn load_dir(&mut self, dir: &Path) -> Vec<LoadError> {
        self.walk(dir, |path| {
            match path.extension().and_then(|ext| ext.to_str()) {
                Some(ext) if ext.eq_ignore_ascii_case("toml") => Some(from_toml as Parse),
                Some(ext) if ext.eq_ignore_ascii_case("xml") => Some(from_udl as Parse),
                _ => None,
            }
        })
    }

    /// Loads only `*.xml` UDL files from `dir`, ignoring every other file.
    /// Returns one error per unreadable entry or failed file; later files win.
    pub fn load_udl_dir(&mut self, dir: &Path) -> Vec<LoadError> {
        self.walk(dir, |path| {
            let is_xml = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"));
            is_xml.then_some(from_udl as Parse)
        })
    }

    /// Adds the languages `parse_for` selects under `dir`, sorted by file name.
    /// A failed `read_dir` entry becomes an error carrying the folder path.
    fn walk(&mut self, dir: &Path, parse_for: impl Fn(&Path) -> Option<Parse>) -> Vec<LoadError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => return vec![io_error(dir, &error)],
        };
        let mut paths = Vec::new();
        let mut errors = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => paths.push(entry.path()),
                Err(error) => errors.push(io_error(dir, &error)),
            }
        }
        paths.sort();
        for path in paths {
            let Some(parse) = parse_for(&path) else {
                continue;
            };
            let loaded = read_rule_file(&path).and_then(|text| parse(&text));
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

/// Reads a regular rule file of at most `MAX_RULE_FILE_BYTES`; anything else is an error.
/// The type and size are checked on the path first (so a FIFO is never opened), then again on the
/// open handle, and the read itself is capped in case the file grows.
fn read_rule_file(path: &Path) -> Result<String, RulesError> {
    let io = |error: std::io::Error| RulesError::Io {
        message: error.to_string(),
    };
    check_regular_and_small(&fs::metadata(path).map_err(io)?)?;
    let file = File::open(path).map_err(io)?;
    check_regular_and_small(&file.metadata().map_err(io)?)?;
    read_capped(file, MAX_RULE_FILE_BYTES).map_err(io)
}

/// Fails unless `meta` describes a regular file within the size cap.
fn check_regular_and_small(meta: &fs::Metadata) -> Result<(), RulesError> {
    if !meta.is_file() {
        return Err(RulesError::Io {
            message: "not a regular file".to_owned(),
        });
    }
    if meta.len() > MAX_RULE_FILE_BYTES {
        return Err(RulesError::Io {
            message: format!("file too large (limit {MAX_RULE_FILE_BYTES} bytes)"),
        });
    }
    Ok(())
}

/// Reads UTF-8 text from `reader`, failing with "too large" when it yields more than `cap` bytes.
fn read_capped(reader: impl Read, cap: u64) -> std::io::Result<String> {
    let mut text = String::new();
    reader
        .take(cap.saturating_add(1))
        .read_to_string(&mut text)?;
    if text.len() as u64 > cap {
        return Err(std::io::Error::other(format!(
            "file too large (limit {cap} bytes)"
        )));
    }
    Ok(text)
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
