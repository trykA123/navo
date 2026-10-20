use std::collections::HashSet;
use std::path::Path;

use rusty_core::{AppState, Command, Direction, Location, PaneId, apply};
use rusty_store::{SessionFileError, load_session, write_session};

const ROWS_PER_PANE: usize = 200;

pub fn initial_state(session_file: &Path, home: Location) -> AppState {
    let (session, _, errors) = load_session(session_file);
    if let Some((found, supported)) = errors.iter().find_map(|error| match error {
        SessionFileError::TooNew { found, supported } => Some((found, supported)),
        _ => None,
    }) {
        let outcome = if session.is_some() {
            "using the backup"
        } else {
            "starting fresh"
        };
        tracing::warn!(
            found,
            supported,
            outcome,
            "session is from a newer Ferrite; the file is kept as it is"
        );
    }
    session
        .and_then(|session| AppState::from_session(session).ok())
        .unwrap_or_else(|| fresh(home))
}

pub fn save(state: &AppState, session_file: &Path) -> Result<(), SessionFileError> {
    write_session(session_file, &state.to_session(ROWS_PER_PANE))
}

pub fn session_folders(state: &AppState) -> Vec<Location> {
    let mut seen = HashSet::new();
    state
        .window_ids()
        .into_iter()
        .filter_map(|id| state.window(id))
        .flat_map(|window| window.panes())
        .flat_map(|pane| pane.tabs())
        .map(|tab| tab.location())
        .filter(|location| seen.insert(*location))
        .cloned()
        .collect()
}

pub fn home_location(var: impl Fn(&str) -> Option<String>) -> Location {
    ["USERPROFILE", "HOME"]
        .iter()
        .filter_map(|name| var(name))
        .find_map(|value| Location::new(value).ok())
        .unwrap_or_else(default_root)
}

pub(crate) fn place_panes(state: &mut AppState, folders: [Option<Location>; 2]) {
    if folders.iter().all(Option::is_none) {
        return;
    }
    let panes = pane_ids(state);
    for (pane, folder) in panes.iter().zip(folders) {
        if let Some(location) = folder {
            focus(state, *pane);
            let _ = apply(state, &Command::PaneNavigate { location });
        }
    }
    if let Some(first) = panes.first() {
        focus(state, *first);
    }
}

pub(crate) fn path_location(path: &Path) -> Option<Location> {
    let path = std::path::absolute(path).ok()?;
    Location::new(path.to_str()?).ok()
}

fn pane_ids(state: &AppState) -> Vec<PaneId> {
    state
        .window(state.focused_window())
        .map(|window| window.panes().iter().map(|pane| pane.id()).collect())
        .unwrap_or_default()
}

fn focus(state: &mut AppState, pane: PaneId) {
    for direction in Direction::ALL {
        if state.focused_pane() == pane {
            return;
        }
        let _ = apply(state, &Command::LayoutFocus { direction });
    }
}

fn fresh(home: Location) -> AppState {
    let mut state = AppState::new(home);
    let _ = apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    state
}

fn default_root() -> Location {
    let root = if cfg!(windows) { r"C:\" } else { "/" };
    Location::new(root).unwrap_or_else(|_| Location::provider_root(rusty_core::ProviderId::local()))
}

#[cfg(test)]
mod tests;
