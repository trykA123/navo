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
pub struct LockHolder {
    pub name: String,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError {
    pub kind: FsErrorKind,
    pub location: Option<Location>,
    pub os_code: Option<i32>,
    pub holders: Vec<LockHolder>,
}

impl FsErrorKind {
    #[must_use]
    pub fn from_windows_code(code: u32) -> Self {
        match code {
            2 | 3 => Self::NotFound,
            5 => Self::AccessDenied,
            32 => Self::SharingViolation,
            33 => Self::Locked,
            39 | 112 => Self::DiskFull,
            111 | 206 => Self::PathTooLong,
            123 | 267 => Self::NameInvalid,
            53 | 55 | 59 | 64 | 67 | 1231 | 1232 | 1236 | 2250 => Self::NetworkLost,
            121 | 1460 => Self::Timeout,
            1223 => Self::Cancelled,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub fn from_errno(code: i32) -> Self {
        match code {
            2 => Self::NotFound,
            1 | 13 => Self::AccessDenied,
            26 => Self::SharingViolation,
            16 => Self::Locked,
            28 => Self::DiskFull,
            36 => Self::PathTooLong,
            20 | 22 | 84 => Self::NameInvalid,
            101 | 103 | 104 | 112 | 113 => Self::NetworkLost,
            110 => Self::Timeout,
            125 => Self::Cancelled,
            _ => Self::Unknown,
        }
    }
}

impl FsError {
    #[must_use]
    pub fn new(kind: FsErrorKind) -> Self {
        Self {
            kind,
            location: None,
            os_code: None,
            holders: Vec::new(),
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        if let Some(message) = self.holder_message() {
            return message;
        }
        let base = match &self.location {
            Some(location) => with_location(self.kind, &location.path().display()),
            None => without_location(self.kind).to_owned(),
        };
        match (self.kind, self.os_code) {
            (FsErrorKind::Unknown, Some(code)) => format!("{base} (error {code})"),
            _ => base,
        }
    }

    #[must_use]
    pub fn recovery(&self) -> &'static [RecoveryAction] {
        use RecoveryAction::{Cancel, Reconnect, Rename, Retry, RetryAsAdmin, Skip, SkipAll};
        match self.kind {
            FsErrorKind::NotFound => &[Skip, SkipAll, Cancel],
            FsErrorKind::AccessDenied => &[RetryAsAdmin, Skip, SkipAll, Cancel],
            FsErrorKind::SharingViolation | FsErrorKind::Locked => &[Retry, Skip, SkipAll, Cancel],
            FsErrorKind::DiskFull => &[Retry, Cancel],
            FsErrorKind::PathTooLong | FsErrorKind::NameInvalid => &[Rename, Skip, Cancel],
            FsErrorKind::NetworkLost => &[Reconnect, Retry, Cancel],
            FsErrorKind::Timeout => &[Retry, Cancel],
            FsErrorKind::Cancelled => &[],
            FsErrorKind::Unknown => &[Retry, Skip, Cancel],
        }
    }

    fn holder_message(&self) -> Option<String> {
        if !matches!(
            self.kind,
            FsErrorKind::SharingViolation | FsErrorKind::Locked
        ) {
            return None;
        }
        let mut names = Vec::new();
        for holder in &self.holders {
            if !names.contains(&holder.name.as_str()) {
                names.push(holder.name.as_str());
            }
        }
        let programs = match names.as_slice() {
            [] => return None,
            [a] => (*a).to_owned(),
            [a, b] => format!("{a} and {b}"),
            [a, b, _] => format!("{a}, {b} and 1 more program"),
            [a, b, rest @ ..] => format!("{a}, {b} and {} more programs", rest.len()),
        };
        let subject = self.location.as_ref().map_or_else(
            || "The item".to_owned(),
            |location| format!("“{}”", location.path().display()),
        );
        Some(format!("{subject} is in use by {programs}."))
    }
}

fn with_location(kind: FsErrorKind, place: &str) -> String {
    match kind {
        FsErrorKind::NotFound => format!("\u{201c}{place}\u{201d} was not found."),
        FsErrorKind::AccessDenied => {
            format!("Ferrite is not allowed to open \u{201c}{place}\u{201d}.")
        }
        FsErrorKind::SharingViolation => {
            format!("\u{201c}{place}\u{201d} is in use by another program.")
        }
        FsErrorKind::Locked => format!("\u{201c}{place}\u{201d} is locked."),
        FsErrorKind::DiskFull => {
            format!("There is not enough free space to write \u{201c}{place}\u{201d}.")
        }
        FsErrorKind::PathTooLong => format!("The path \u{201c}{place}\u{201d} is too long."),
        FsErrorKind::NameInvalid => {
            format!("\u{201c}{place}\u{201d} has a name that is not allowed here.")
        }
        FsErrorKind::NetworkLost => format!("The connection to \u{201c}{place}\u{201d} was lost."),
        FsErrorKind::Timeout => format!("\u{201c}{place}\u{201d} did not answer in time."),
        FsErrorKind::Cancelled => {
            format!("The operation on \u{201c}{place}\u{201d} was cancelled.")
        }
        FsErrorKind::Unknown => format!("Something went wrong with \u{201c}{place}\u{201d}."),
    }
}

fn without_location(kind: FsErrorKind) -> &'static str {
    match kind {
        FsErrorKind::NotFound => "The item was not found.",
        FsErrorKind::AccessDenied => "Ferrite is not allowed to do this.",
        FsErrorKind::SharingViolation => "The item is in use by another program.",
        FsErrorKind::Locked => "The item is locked.",
        FsErrorKind::DiskFull => "There is not enough free space on the drive.",
        FsErrorKind::PathTooLong => "The path is too long.",
        FsErrorKind::NameInvalid => "The name is not allowed here.",
        FsErrorKind::NetworkLost => "The network connection was lost.",
        FsErrorKind::Timeout => "The operation took too long.",
        FsErrorKind::Cancelled => "The operation was cancelled.",
        FsErrorKind::Unknown => "Something went wrong.",
    }
}

#[cfg(test)]
mod tests;
