use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use navo_shell::Hive;

use crate::GitError;

const GIT_KEY: &str = r"SOFTWARE\GitForWindows";
const GIT_KEY_32: &str = r"SOFTWARE\WOW6432Node\GitForWindows";
const REGISTRY_KEYS: [(Hive, &str); 3] = [
    (Hive::LocalMachine, GIT_KEY),
    (Hive::CurrentUser, GIT_KEY),
    (Hive::LocalMachine, GIT_KEY_32),
];
/// Environment folder and the Git folder below it, tried after the registry.
const FOLDERS: [(&str, &[&str]); 4] = [
    ("ProgramFiles", &["Git"]),
    ("ProgramW6432", &["Git"]),
    ("ProgramFiles(x86)", &["Git"]),
    ("LOCALAPPDATA", &["Programs", "Git"]),
];

/// Everything the search reads from the machine, injected so tests need no Windows.
pub(super) struct Sources<'a> {
    pub windows: bool,
    pub path: Option<OsString>,
    pub var: &'a dyn Fn(&str) -> Option<OsString>,
    pub registry: &'a dyn Fn(Hive, &str, &str) -> Option<String>,
    /// `Ok(true)` for a file, `Ok(false)` for a missing path or a folder.
    pub is_file: &'a dyn Fn(&Path) -> io::Result<bool>,
}

/// Finds `git` on PATH. On Windows it then tries the Git for Windows registry keys and the
/// usual install folders. Those extra candidates must be absolute and outside `repo`.
pub(super) fn find_git(repo: &Path) -> Result<PathBuf, GitError> {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    let registry = |hive: Hive, key: &str, value: &str| navo_shell::registry_text(hive, key, value);
    locate(
        &Sources {
            windows: cfg!(windows),
            path: std::env::var_os("PATH"),
            var: &var,
            registry: &registry,
            is_file: &file_exists,
        },
        repo,
    )
}

fn file_exists(path: &Path) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn locate(sources: &Sources<'_>, repo: &Path) -> Result<PathBuf, GitError> {
    if let Some(found) = on_path(sources)? {
        return Ok(found);
    }
    if sources.windows {
        for candidate in install_candidates(sources) {
            let usable = candidate.is_absolute()
                && !candidate.starts_with(repo)
                && (sources.is_file)(&candidate).unwrap_or(false);
            if usable {
                return Ok(candidate);
            }
        }
    }
    Err(GitError::GitNotFound)
}

fn on_path(sources: &Sources<'_>) -> Result<Option<PathBuf>, GitError> {
    let name = if sources.windows { "git.exe" } else { "git" };
    let Some(paths) = &sources.path else {
        return Ok(None);
    };
    for folder in std::env::split_paths(paths) {
        let candidate = folder.join(name);
        match (sources.is_file)(&candidate) {
            Ok(true) => return Ok(Some(candidate)),
            Ok(false) => {}
            Err(error) => {
                return Err(GitError::Io {
                    path: candidate,
                    kind: error.kind(),
                });
            }
        }
    }
    Ok(None)
}

/// Registry install folders first (plain string values only), then the usual folders.
fn install_candidates(sources: &Sources<'_>) -> Vec<PathBuf> {
    let from_registry = REGISTRY_KEYS
        .iter()
        .filter_map(|&(hive, key)| (sources.registry)(hive, key, "InstallPath"))
        .map(|text| PathBuf::from(text.trim()))
        .filter(|base| !base.as_os_str().is_empty());
    let from_folders = FOLDERS.iter().filter_map(|(variable, below)| {
        let mut base = PathBuf::from((sources.var)(variable)?);
        base.extend(*below);
        Some(base)
    });
    from_registry
        .chain(from_folders)
        .map(|base| base.join("cmd").join("git.exe"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    /// Test paths are written Unix-style; on Windows they get a drive so they are absolute there too.
    fn abs(text: &str) -> PathBuf {
        match (cfg!(windows), text.starts_with('/')) {
            (true, true) => PathBuf::from(format!("C:{text}")),
            _ => PathBuf::from(text),
        }
    }

    #[derive(Default)]
    struct Machine {
        windows: bool,
        path: Option<OsString>,
        vars: HashMap<&'static str, &'static str>,
        registry: Vec<((Hive, &'static str), &'static str)>,
        files: HashSet<PathBuf>,
        broken: HashSet<PathBuf>,
    }

    impl Machine {
        fn windows() -> Self {
            Self {
                windows: true,
                ..Self::default()
            }
        }

        fn is_file(&self, path: &Path) -> io::Result<bool> {
            if self.broken.contains(path) {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            Ok(self.files.contains(path))
        }

        fn find(&self, repo: &str) -> Result<PathBuf, GitError> {
            let var = |name: &str| self.vars.get(name).map(|text| abs(text).into_os_string());
            let registry = |hive: Hive, key: &str, value: &str| {
                assert_eq!(value, "InstallPath", "only InstallPath is read");
                let found = self
                    .registry
                    .iter()
                    .find(|((h, k), _)| *h == hive && *k == key);
                found.map(|(_, text)| abs(text).to_string_lossy().into_owned())
            };
            let is_file = |path: &Path| self.is_file(path);
            locate(
                &Sources {
                    windows: self.windows,
                    path: self.path.clone(),
                    var: &var,
                    registry: &registry,
                    is_file: &is_file,
                },
                &abs(repo),
            )
        }
    }

    fn git_in(base: &str) -> PathBuf {
        abs(base).join("cmd").join("git.exe")
    }

    #[test]
    fn git_off_path_is_found_through_the_registry() {
        let mut machine = Machine::windows();
        machine
            .registry
            .push(((Hive::LocalMachine, GIT_KEY), "/apps/Git"));
        machine.files.insert(git_in("/apps/Git"));
        assert_eq!(machine.find("/work/repo").unwrap(), git_in("/apps/Git"));
    }

    #[test]
    fn each_registry_view_is_tried() {
        for (hive, key) in REGISTRY_KEYS {
            let mut machine = Machine::windows();
            machine.registry.push(((hive, key), "/apps/Git"));
            machine.files.insert(git_in("/apps/Git"));
            assert_eq!(machine.find("/work/repo").unwrap(), git_in("/apps/Git"));
        }
    }

    #[test]
    fn program_files_is_used_when_the_registry_has_nothing() {
        let mut machine = Machine::windows();
        machine.vars.insert("ProgramFiles", "/pf");
        machine.vars.insert("LOCALAPPDATA", "/local");
        machine.files.insert(git_in("/local/Programs/Git"));
        assert_eq!(
            machine.find("/work/repo").unwrap(),
            git_in("/local/Programs/Git")
        );
        machine.files.insert(git_in("/pf/Git"));
        assert_eq!(machine.find("/work/repo").unwrap(), git_in("/pf/Git"));
    }

    #[test]
    fn the_registry_beats_the_install_folders() {
        let mut machine = Machine::windows();
        machine
            .registry
            .push(((Hive::CurrentUser, GIT_KEY), "/apps/Git"));
        machine.vars.insert("ProgramFiles", "/pf");
        machine.files.insert(git_in("/apps/Git"));
        machine.files.insert(git_in("/pf/Git"));
        assert_eq!(machine.find("/work/repo").unwrap(), git_in("/apps/Git"));
    }

    #[test]
    fn path_wins_over_the_registry_and_the_folders() {
        let mut machine = Machine::windows();
        machine.path = Some(std::env::join_paths([abs("/missing"), abs("/on/path")]).unwrap());
        machine
            .registry
            .push(((Hive::LocalMachine, GIT_KEY), "/apps/Git"));
        machine.vars.insert("ProgramFiles", "/pf");
        machine.files.insert(abs("/on/path").join("git.exe"));
        machine.files.insert(git_in("/apps/Git"));
        machine.files.insert(git_in("/pf/Git"));
        assert_eq!(
            machine.find("/work/repo").unwrap(),
            abs("/on/path").join("git.exe")
        );
    }

    #[test]
    fn a_registry_value_for_a_missing_file_is_skipped() {
        let mut machine = Machine::windows();
        machine
            .registry
            .push(((Hive::LocalMachine, GIT_KEY), "/gone/Git"));
        machine.vars.insert("ProgramFiles", "/pf");
        machine.files.insert(git_in("/pf/Git"));
        assert_eq!(machine.find("/work/repo").unwrap(), git_in("/pf/Git"));
    }

    #[test]
    fn a_folder_or_a_missing_path_is_not_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("git.exe");
        fs::write(&file, b"").unwrap();
        let folder = dir.path().join("folder");
        fs::create_dir(&folder).unwrap();
        assert!(file_exists(&file).unwrap());
        assert!(!file_exists(&folder).unwrap());
        assert!(!file_exists(&dir.path().join("none")).unwrap());
    }

    #[test]
    fn nothing_found_is_git_not_found() {
        assert!(matches!(
            Machine::windows().find("/work/repo"),
            Err(GitError::GitNotFound)
        ));
    }

    #[test]
    fn relative_and_blank_registry_values_are_refused() {
        let mut machine = Machine::windows();
        machine
            .registry
            .push(((Hive::LocalMachine, GIT_KEY), "Git"));
        machine.registry.push(((Hive::CurrentUser, GIT_KEY), "   "));
        machine.files.insert(git_in("Git"));
        assert!(matches!(
            machine.find("/work/repo"),
            Err(GitError::GitNotFound)
        ));
    }

    #[test]
    fn a_git_inside_the_repository_is_refused() {
        let mut machine = Machine::windows();
        machine
            .registry
            .push(((Hive::LocalMachine, GIT_KEY), "/work/repo/Git"));
        machine.files.insert(git_in("/work/repo/Git"));
        assert!(matches!(
            machine.find("/work/repo"),
            Err(GitError::GitNotFound)
        ));
    }

    #[test]
    fn other_systems_search_path_only() {
        let mut machine = Machine::default();
        machine
            .registry
            .push(((Hive::LocalMachine, GIT_KEY), "/apps/Git"));
        machine.vars.insert("ProgramFiles", "/pf");
        machine.files.insert(git_in("/apps/Git"));
        machine.files.insert(git_in("/pf/Git"));
        assert!(matches!(
            machine.find("/work/repo"),
            Err(GitError::GitNotFound)
        ));
        machine.path = Some(abs("/bin").into_os_string());
        machine.files.insert(abs("/bin").join("git"));
        assert_eq!(machine.find("/work/repo").unwrap(), abs("/bin").join("git"));
    }

    #[test]
    fn a_path_entry_that_cannot_be_read_is_an_io_error() {
        let mut machine = Machine::windows();
        machine.path = Some(abs("/locked").into_os_string());
        machine.broken.insert(abs("/locked").join("git.exe"));
        assert!(matches!(
            machine.find("/work/repo"),
            Err(GitError::Io { .. })
        ));
    }
}
