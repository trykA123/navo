#![forbid(unsafe_code)]

mod event_ring;
mod redact;

pub use event_ring::{EventRing, RingStats};
pub use redact::{DriveKind, Identity, LengthClass, PathKind, RedactedPath, Redactor, Salt};
