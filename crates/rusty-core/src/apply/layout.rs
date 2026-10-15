#![allow(
    dead_code,
    unused_variables,
    reason = "stubs until apply is implemented"
)]

use super::{CommandError, Effect};
use crate::{AppState, Direction};

pub(super) fn split(
    state: &mut AppState,
    direction: Direction,
) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn close(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn focus(
    state: &mut AppState,
    direction: Direction,
) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn save_session(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}
