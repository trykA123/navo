use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};

use navo_ui::browser::LoggedOperation;

use super::mutation_lock::{LeaseError, Mode, WAIT, Wait, wait_for};

type Profiles = BTreeMap<PathBuf, Weak<Mutex<State>>>;

static PROFILES: LazyLock<Mutex<Profiles>> = LazyLock::new(Mutex::default);

#[derive(Debug, Default)]
struct State {
    jobs: usize,
    restoring: bool,
}

/// Owns admission until the mutation and its cleanup finish.
#[derive(Debug)]
pub(super) struct Hold {
    state: Arc<Mutex<State>>,
    mode: Mode,
}

pub(super) fn acquire(data: &Path, mode: Mode, wait: Wait<'_>) -> Result<Hold, LeaseError> {
    let state = {
        let mut profiles = PROFILES.lock().unwrap_or_else(PoisonError::into_inner);
        profiles.retain(|_, state| state.strong_count() > 0);
        let entry = profiles.entry(data.to_path_buf()).or_default();
        match entry.upgrade() {
            Some(state) => state,
            None => {
                let state = Arc::new(Mutex::new(State::default()));
                *entry = Arc::downgrade(&state);
                state
            }
        }
    };
    wait_for(wait, || {
        let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.restoring || matches!(mode, Mode::Exclusive) && state.jobs > 0 {
            return Ok(false);
        }
        match mode {
            Mode::Shared => state.jobs += 1,
            Mode::Exclusive => state.restoring = true,
        }
        Ok(true)
    })?;
    Ok(Hold { state, mode })
}

pub(super) fn operation(data: &Path, operation: &mut LoggedOperation<'_>) -> Result<Hold, String> {
    acquire(
        data,
        Mode::Shared,
        Wait {
            cancel: Some(operation.cancel),
            waiting: Some(&mut *operation.waiting),
            limit: WAIT,
        },
    )
    .map_err(String::from)
}

impl Drop for Hold {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match self.mode {
            Mode::Shared => state.jobs -= 1,
            Mode::Exclusive => state.restoring = false,
        }
    }
}
