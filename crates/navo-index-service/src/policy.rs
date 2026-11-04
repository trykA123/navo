pub(super) const RESET_SECONDS: u32 = 86_400;
pub(super) const FAILURE_DELAYS: [Option<u32>; 3] = [Some(5_000), Some(60_000), None];
pub(super) const RESTART_CLEAN_STOPS: bool = false;

#[derive(Debug, thiserror::Error)]
pub(super) enum ServiceError {
    #[error("service configuration: {0}")]
    Configuration(std::io::Error),
    #[error("service storage: {0}")]
    Storage(std::io::Error),
    #[error("unexpected service fault: {0}")]
    Unexpected(std::io::Error),
}

impl ServiceError {
    pub fn stop_code(&self) -> Option<u32> {
        match self {
            Self::Configuration(_) => Some(1),
            Self::Storage(_) => Some(2),
            Self::Unexpected(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_restart_twice_then_stop_for_one_day() {
        assert_eq!(
            (RESET_SECONDS, FAILURE_DELAYS, RESTART_CLEAN_STOPS),
            (86_400, [Some(5_000), Some(60_000), None], false),
        );
    }

    #[test]
    fn setup_errors_stop_cleanly_and_unexpected_errors_restart() {
        for error in [
            ServiceError::Configuration(std::io::Error::other("bad config")),
            ServiceError::Storage(std::io::Error::other("bad ACL")),
        ] {
            assert!(error.stop_code().is_some_and(|code| code != 0));
        }
        assert_eq!(
            ServiceError::Unexpected(std::io::Error::other("panic")).stop_code(),
            None
        );
    }
}
