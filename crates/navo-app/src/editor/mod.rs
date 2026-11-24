use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};

use navo_core::Location;
use navo_ui::window_view::EditorHooks;

const WINDOWS_CANDIDATES: [(&str, &str); 4] = [
    ("LOCALAPPDATA", r"Programs\Microsoft VS Code\Code.exe"),
    ("ProgramFiles", r"Microsoft VS Code\Code.exe"),
    ("ProgramFiles", r"Notepad++\notepad++.exe"),
    ("SystemRoot", "notepad.exe"),
];
const DEFAULT_EXTENSIONS: &str = ".COM;.EXE;.BAT;.CMD";

pub struct Search {
    pub path: String,
    pub extensions: String,
}

pub fn clean(program: &str) -> String {
    program.trim().trim_matches('"').trim().to_owned()
}

const UNIX_CANDIDATES: [&str; 3] = ["/usr/bin/code", "/usr/bin/nvim", "/usr/bin/vim"];

pub fn hooks(var: impl Fn(&str) -> Option<String>) -> EditorHooks {
    let search = Search {
        path: var("PATH").unwrap_or_default(),
        extensions: var("PATHEXT").unwrap_or_else(|| DEFAULT_EXTENSIONS.to_owned()),
    };
    let candidates = candidates(&var);
    EditorHooks {
        suggestion: Arc::new(suggest(candidates)),
        resolve: Arc::new(move |program| found(program, &search).map_err(EditorError::message)),
        open: Arc::new(|program, file| open(&clean(program), file).map_err(EditorError::message)),
    }
}

fn candidates(var: &impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    if cfg!(windows) {
        WINDOWS_CANDIDATES
            .iter()
            .filter_map(|(root, rest)| Some(PathBuf::from(var(root)?).join(rest)))
            .collect()
    } else {
        UNIX_CANDIDATES.iter().map(PathBuf::from).collect()
    }
}

fn suggest(candidates: Vec<PathBuf>) -> impl Fn() -> Option<String> + Send + Sync {
    let found = OnceLock::new();
    move || {
        found
            .get_or_init(|| {
                candidates
                    .iter()
                    .find(|path| path.is_file())
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .clone()
    }
}

pub fn resolve(program: &str, search: &Search) -> Option<PathBuf> {
    let path = Path::new(program);
    if program.is_empty() {
        return None;
    }
    if path.is_file() {
        return Some(path.to_owned());
    }
    if path.components().count() != 1 {
        return None;
    }
    let with_extensions = search
        .extensions
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| format!("{program}{}", ext.to_lowercase()));
    let bare = (!cfg!(windows) || path.extension().is_some()).then(|| program.to_owned());
    let names: Vec<String> = bare.into_iter().chain(with_extensions).collect();
    std::env::split_paths(&search.path).find_map(|folder| {
        names
            .iter()
            .map(|name| folder.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// Why the editor launcher failed; the hooks turn each case into a localized message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorError {
    /// The chosen program is not a file and not on the search path.
    NoProgram,
    /// The file is not on a local disk, so no editor can open it.
    NotLocal,
    /// The operating system refused to start the program.
    Spawn(io::ErrorKind),
}

impl EditorError {
    /// The en-US (or current locale) text for this failure, ending with the next step.
    pub fn message(self) -> String {
        match self {
            Self::NoProgram => navo_l10n::tr("editor-no-program"),
            Self::NotLocal => navo_l10n::tr("editor-not-local"),
            Self::Spawn(kind) => navo_l10n::t!("editor-spawn-error", error = kind.to_string()),
        }
    }
}

/// Resolves a chosen program to its full path; fails with `NoProgram` when nothing matches.
fn found(program: &str, search: &Search) -> Result<String, EditorError> {
    resolve(&clean(program), search)
        .map(|found| found.to_string_lossy().into_owned())
        .ok_or(EditorError::NoProgram)
}

/// Starts `program` with the local `file` as its argument; fails for remote files or a failed spawn.
fn open(program: &str, file: &Location) -> Result<(), EditorError> {
    if !file.is_local() {
        return Err(EditorError::NotLocal);
    }
    Command::new(program)
        .arg(file.as_str())
        .spawn()
        .map(|_| ())
        .map_err(|error| EditorError::Spawn(error.kind()))
}

#[cfg(test)]
mod tests;
