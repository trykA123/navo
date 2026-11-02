#[derive(Debug, thiserror::Error)]
pub(crate) enum ElevationError {
    #[error("index service elevation was cancelled")]
    Cancelled,
    #[error("index service operation failed (exit code {0})")]
    Failed(u32),
    #[error("index service elevation: {0}")]
    Launch(#[from] std::io::Error),
}

pub(super) fn from_exit_code(code: u32) -> Result<(), ElevationError> {
    match code {
        0 => Ok(()),
        code => Err(ElevationError::Failed(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_child_exit_code_is_not_lost() {
        assert!(from_exit_code(0).is_ok());
        assert!(matches!(from_exit_code(7), Err(ElevationError::Failed(7))));
        assert!(!ElevationError::Cancelled.to_string().is_empty());
    }
}
