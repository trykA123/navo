//! Limits derived from the install and remove steps for the elevated child wait.

use std::time::Duration;

use crate::REMOVE_DEADLINE;
use crate::removal::{INSTALL_STOP_LIMIT, PROCESS_WAIT};

pub(super) const CHANGE_WAIT: Duration = Duration::from_secs(60);
pub(super) const ANSWER_WAIT: Duration = Duration::from_secs(30);
pub(super) const ANSWER_TRY: Duration = Duration::from_secs(2);
pub(super) const ELEVATION_WAIT: Duration = Duration::from_secs(
    CHANGE_WAIT.as_secs()
        + 2 * (INSTALL_STOP_LIMIT.as_secs() + PROCESS_WAIT.as_secs())
        + 2 * INSTALL_STOP_LIMIT.as_secs()
        + 2 * (ANSWER_WAIT.as_secs() + ANSWER_TRY.as_secs())
        + ANSWER_TRY.as_secs()
        + 2 * REMOVE_DEADLINE.as_secs()
        + 10,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevation_wait_covers_the_step_limits_and_rollback() {
        let steps = CHANGE_WAIT
            + (INSTALL_STOP_LIMIT + PROCESS_WAIT) * 2
            + INSTALL_STOP_LIMIT * 2
            + (ANSWER_WAIT + ANSWER_TRY) * 2
            + ANSWER_TRY
            + REMOVE_DEADLINE * 2;
        assert!(ELEVATION_WAIT >= steps);
        assert!(INSTALL_STOP_LIMIT >= Duration::from_secs(35));
    }
}
