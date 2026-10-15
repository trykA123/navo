#![forbid(unsafe_code)]

mod event_ring;
mod memory;
mod redact;

pub use event_ring::{EventRing, RingStats};
pub use memory::{
    CacheHandle, CacheRegistry, CacheUsage, FakeProbe, MemoryProbe, MemoryReport, ProcessMemory,
    TaskCounters, TaskGuard, memory_report,
};
#[cfg(target_os = "linux")]
pub use memory::{ProcStatusProbe, parse_proc_status};
pub use redact::{DriveKind, Identity, LengthClass, PathKind, RedactedPath, Redactor, Salt};
