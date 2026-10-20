use std::path::PathBuf;

use super::{DragError, DropOutcome};

pub(super) fn drag_files(_hwnd: isize, _paths: &[PathBuf]) -> Result<DropOutcome, DragError> {
    Err(DragError::Unsupported)
}
