//! Loads rule languages from the UDL and Navo rule folders.
//! It keeps D14's folder policy out of the registry that `syntax_rules` installs.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use navo_syntax::{LoadError, Registry, RulesError};

/// One folder of rule files and the parser its files use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleDir {
    Udl(PathBuf),
    Own(PathBuf),
}

/// Loads built-ins then every folder in order; later folders win on a name.
pub fn load_rules(dirs: &[RuleDir]) -> (Registry, Vec<LoadError>) {
    let mut registry = Registry::builtin();
    let mut errors = Vec::new();
    for dir in dirs {
        let (path, udl) = match dir {
            RuleDir::Udl(path) => (path, true),
            RuleDir::Own(path) => (path, false),
        };
        match fs::metadata(path) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                errors.push(not_a_folder(path));
                continue;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                errors.push(io_error(path, &error));
                continue;
            }
        }
        let found = if udl {
            registry.load_udl_dir(path)
        } else {
            registry.load_dir(path)
        };
        errors.extend(found);
    }
    (registry, errors)
}

/// Builds the single error for a path that exists but is not a folder.
fn not_a_folder(path: &Path) -> LoadError {
    LoadError {
        path: path.to_path_buf(),
        error: RulesError::Io {
            message: "not a folder".to_owned(),
        },
    }
}

/// Wraps a folder metadata error with the path it came from.
fn io_error(path: &Path, error: &std::io::Error) -> LoadError {
    LoadError {
        path: path.to_path_buf(),
        error: RulesError::Io {
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use navo_syntax::Language;
    use tempfile::TempDir;

    use super::*;

    /// Builds a one-language rule TOML with a unique keyword.
    fn language_toml(name: &str, ext: &str, word: &str) -> String {
        format!(
            "[[language]]\nname = \"{name}\"\nextensions = [\"{ext}\"]\n\
             [[language.keywords]]\nkind = \"keyword\"\nwords = [\"{word}\"]\n"
        )
    }

    /// Builds a one-language UDL XML file with a unique keyword.
    fn language_udl(name: &str, ext: &str, word: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n\
             <NotepadPlus>\n\
             \x20   <UserLang name=\"{name}\" ext=\"{ext}\" udlVersion=\"2.1\">\n\
             \x20       <Settings><Global caseIgnored=\"no\" /></Settings>\n\
             \x20       <KeywordLists><Keywords name=\"Keywords1\">{word}</Keywords></KeywordLists>\n\
             \x20   </UserLang>\n\
             </NotepadPlus>\n"
        )
    }

    /// Writes `text` under `dir` as `name`.
    fn write(dir: &Path, name: &str, text: &str) {
        fs::write(dir.join(name), text).unwrap();
    }

    /// True when the language defines `word` in any keyword group.
    fn has_word(language: &Language, word: &str) -> bool {
        language
            .keywords
            .iter()
            .any(|group| group.words.iter().any(|found| found == word))
    }

    /// Returns a fresh temporary root for the folder cases.
    fn root() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn later_folders_win_over_builtins_and_udl() {
        let temp = root();
        let udl_dir = temp.path().join("udl");
        let own_dir = temp.path().join("own");
        fs::create_dir_all(&udl_dir).unwrap();
        fs::create_dir_all(&own_dir).unwrap();
        write(&udl_dir, "oil.xml", &language_udl("oil", "oil", "UDL_OIL"));
        write(
            &own_dir,
            "oil.toml",
            &language_toml("oil", "oil", "OWN_OIL"),
        );
        let (udl, errors) = load_rules(&[RuleDir::Udl(udl_dir.clone())]);
        assert!(errors.is_empty(), "{errors:?}");
        let oil = udl.get("oil").expect("UDL oil");
        assert!(has_word(oil, "UDL_OIL"));
        assert!(!has_word(oil, "TASK"));
        let (registry, errors) = load_rules(&[RuleDir::Udl(udl_dir), RuleDir::Own(own_dir)]);
        assert!(errors.is_empty(), "{errors:?}");
        let oil = registry.get("oil").expect("own oil");
        assert!(has_word(oil, "OWN_OIL"));
        assert!(!has_word(oil, "UDL_OIL"));
    }

    #[test]
    fn missing_folders_are_silent() {
        let temp = root();
        let missing = temp.path().join("missing");
        let dirs = [RuleDir::Udl(missing.clone()), RuleDir::Own(missing)];
        let (registry, errors) = load_rules(&dirs);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(registry.get("oil").is_some(), "built-ins still load");
    }

    #[test]
    fn a_file_where_a_folder_should_be_is_one_error() {
        let temp = root();
        let file = temp.path().join("not-a-folder.txt");
        fs::write(&file, "x").unwrap();
        let (_, errors) = load_rules(&[RuleDir::Own(file.clone())]);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].path, file);
    }

    #[test]
    fn a_folder_with_a_unicode_name_loads() {
        let temp = root();
        let dir = temp.path().join("Prüfung-ü");
        fs::create_dir(&dir).unwrap();
        write(
            &dir,
            "demo.toml",
            &language_toml("prüfung", "pruefung", "PRUEFUNG"),
        );
        let (registry, errors) = load_rules(&[RuleDir::Own(dir)]);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(registry.get("prüfung").is_some());
    }

    #[test]
    fn crlf_toml_and_crlf_udl_load() {
        let temp = root();
        let own_dir = temp.path().join("own");
        let udl_dir = temp.path().join("udl");
        fs::create_dir_all(&own_dir).unwrap();
        fs::create_dir_all(&udl_dir).unwrap();
        let toml = language_toml("crlf", "crlf", "CRLF_TOML").replace('\n', "\r\n");
        write(&own_dir, "crlf.toml", &toml);
        let xml = language_udl("crlfudl", "crlfudl", "CRLF_UDL").replace('\n', "\r\n");
        write(&udl_dir, "crlf.xml", &xml);
        let (registry, errors) = load_rules(&[RuleDir::Own(own_dir), RuleDir::Udl(udl_dir)]);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(registry.get("crlf").is_some());
        assert!(registry.get("crlfudl").is_some());
    }
}
