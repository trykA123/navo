/// How an elevated run that did not fail ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elevated {
    Done,
    /// The service is removed and Windows deletes its last files at the next restart.
    Deferred,
}

#[derive(Debug, thiserror::Error)]
pub enum ElevationError {
    #[error("index service elevation was cancelled")]
    Cancelled,
    #[error("index service operation failed: {reason} (exit code {0})", reason = reason(*.0))]
    Failed(u32),
    #[error("index service image {} is locked; close the program using it and retry", .0.display())]
    LockedImage(std::path::PathBuf),
    #[error("index service elevation: {0}")]
    Launch(#[from] std::io::Error),
}

const NOT_REPLACED: u8 = 3;
const TIMED_OUT: u8 = 4;
const DENIED: u8 = 5;
const DEFERRED: u8 = 6;

fn reason(code: u32) -> &'static str {
    match u8::try_from(code) {
        Ok(NOT_REPLACED) => "the service does not run the new image, so the old one was kept",
        Ok(TIMED_OUT) => "the service did not stop or start in time",
        Ok(DENIED) => "access was denied",
        Ok(DEFERRED) => {
            "the service is removed; Windows deletes its last files at the next restart"
        }
        _ => "the elevated run's output has the details",
    }
}
