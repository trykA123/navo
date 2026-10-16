use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use rusty_core::Location;
use rusty_ui::window_view::EditorHooks;

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
    EditorHooks {
        suggestion: suggest(&var),
        resolve: Rc::new(move |program| found(program, &search)),
        open: Rc::new(|program, file| open(&clean(program), file)),
    }
}

fn suggest(var: &impl Fn(&str) -> Option<String>) -> Option<String> {
    let found = if cfg!(windows) {
        WINDOWS_CANDIDATES
            .iter()
            .filter_map(|(root, rest)| Some(PathBuf::from(var(root)?).join(rest)))
            .find(|path| path.is_file())
    } else {
        UNIX_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
    };
    found.map(|path| path.to_string_lossy().into_owned())
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

fn found(program: &str, search: &Search) -> Result<String, String> {
    resolve(&clean(program), search)
        .map(|found| found.to_string_lossy().into_owned())
        .ok_or_else(|| "No program at that path.".to_owned())
}

fn open(program: &str, file: &Location) -> Result<(), String> {
    if !file.is_local() {
        return Err("Only local files open in an editor.".to_owned());
    }
    Command::new(program)
        .arg(file.as_str())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start {program}: {error}"))
}

#[cfg(test)]
mod tests;
