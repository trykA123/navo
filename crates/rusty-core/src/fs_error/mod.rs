use crate::Location;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FsErrorKind {
    NotFound,
    AccessDenied,
    SharingViolation,
    Locked,
    DiskFull,
    PathTooLong,
    NameInvalid,
    NetworkLost,
    Timeout,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    Retry,
    RetryAsAdmin,
    Skip,
    SkipAll,
    Cancel,
    Rename,
    OpenFolder,
    Reconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError {
    pub kind: FsErrorKind,
    pub location: Option<Location>,
    pub os_code: Option<i32>,
}

impl FsErrorKind {
    #[must_use]
    pub fn from_windows_code(code: u32) -> Self {
        todo!()
    }

    #[must_use]
    pub fn from_errno(code: i32) -> Self {
        todo!()
    }
}

impl FsError {
    #[must_use]
    pub fn message(&self) -> String {
        todo!()
    }

    #[must_use]
    pub fn recovery(&self) -> &'static [RecoveryAction] {
        todo!()
    }
}

#[cfg(test)]
mod tests;
