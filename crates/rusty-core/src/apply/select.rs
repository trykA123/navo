#![allow(
    dead_code,
    unused_variables,
    reason = "stubs until apply is implemented"
)]

use super::{CommandError, Effect};
use crate::AppState;

pub(super) fn toggle(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn toggle_down(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn range(state: &mut AppState, to: u64) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn add_pattern(
    state: &mut AppState,
    pattern: &str,
) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn remove_pattern(
    state: &mut AppState,
    pattern: &str,
) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn invert(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn all(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn clear(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}
