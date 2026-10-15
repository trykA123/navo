use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use rusty_core::Location;
use rusty_store::SettingsStore;
use rusty_ui::window_view::EditorHooks;

const WINDOWS_CANDIDATES: [(&str, &str); 4] = [
    ("LOCALAPPDATA", r"Programs\Microsoft VS Code\Code.exe"),
    ("ProgramFiles", r"Microsoft VS Code\Code.exe"),
    ("ProgramFiles", r"Notepad++\notepad++.exe"),
    ("SystemRoot", "notepad.exe"),
];
const UNIX_CANDIDATES: [&str; 3] = ["/usr/bin/code", "/usr/bin/nvim", "/usr/bin/vim"];

pub fn hooks(settings: Option<&Path>, var: impl Fn(&str) -> Option<String>) -> EditorHooks {
    let store = settings.map(|file| Rc::new(RefCell::new(SettingsStore::load(file).0)));
    let chosen = store
        .as_ref()
        .and_then(|store| store.borrow().current().editor.path.clone());
    let search = var("PATH").unwrap_or_default();
    EditorHooks {
        chosen,
        suggestion: suggest(&var),
        remember: Rc::new(move |program| remember(store.as_ref(), program, &search)),
        open: Rc::new(open),
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

pub fn resolves(program: &str, search: &str) -> bool {
    let path = Path::new(program);
    if path.is_file() {
        return true;
    }
    path.components().count() == 1
        && std::env::split_paths(search).any(|folder| folder.join(program).is_file())
}

fn remember(
    store: Option<&Rc<RefCell<SettingsStore>>>,
    program: &str,
    search: &str,
) -> Result<(), String> {
    let program = program.trim();
    if !resolves(program, search) {
        return Err("No program at that path.".to_owned());
    }
    let Some(store) = store else {
        return Ok(());
    };
    let mut settings = store.borrow().current().clone();
    settings.editor.path = Some(program.to_owned());
    store
        .borrow_mut()
        .save(settings)
        .map_err(|error| format!("Could not save the choice: {error}"))
}

fn open(program: &str, file: &Location) -> Result<(), String> {
    if !file.is_local() {
        return Err("Only local files open in an editor.".to_owned());
    }
    Command::new(program.trim())
        .arg(file.as_str())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not start {program}: {error}"))
}

#[cfg(test)]
mod tests;
