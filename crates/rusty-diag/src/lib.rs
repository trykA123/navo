#![forbid(unsafe_code)]

mod event_ring;
mod log_layer;
mod memory;
mod redact;
mod report;

pub use event_ring::{EventRing, RingStats};
pub use log_layer::{LogConfig, LogLayer, RedactionMisses, is_redacted_path};
pub use memory::{
    CacheRegistry, CacheSize, CacheUsage, FakeProbe, MemoryProbe, MemoryReport, ProcessMemory,
    TaskCounters, TaskGuard, memory_report,
};
#[cfg(target_os = "linux")]
pub use memory::{ProcStatusProbe, parse_proc_status};
pub use redact::{DriveKind, Identity, LengthClass, PathKind, RedactedPath, Redactor, Salt};
pub use report::{
    BuildInfo, DpiAwareness, ErrorRecord, ReportInput, ReportSummary, SystemInfo, write_report,
    write_report_with_crashes,
};
