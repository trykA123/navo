use std::path::Path;

use rusty_core::{AppState, Command, Direction, Location, apply};
use rusty_store::{SessionFileError, load_session, write_session};

const ROWS_PER_PANE: usize = 200;

pub fn initial_state(session_file: &Path, home: Location) -> AppState {
    let (session, _, _) = load_session(session_file);
    session
        .and_then(|session| AppState::from_session(session).ok())
        .unwrap_or_else(|| fresh(home))
}

pub fn save(state: &AppState, session_file: &Path) -> Result<(), SessionFileError> {
    write_session(session_file, &state.to_session(ROWS_PER_PANE))
}

pub fn home_location(var: impl Fn(&str) -> Option<String>) -> Location {
    ["USERPROFILE", "HOME"]
        .iter()
        .filter_map(|name| var(name))
        .find_map(|value| Location::new(value).ok())
        .unwrap_or_else(default_root)
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
