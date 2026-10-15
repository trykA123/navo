#![forbid(unsafe_code)]

mod redact;

pub use redact::{DriveKind, Identity, LengthClass, PathKind, RedactedPath, Redactor, Salt};
