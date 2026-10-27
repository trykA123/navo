#![forbid(unsafe_code)]

pub mod components;
mod disk_use;
mod event_ring;
pub mod heap;
mod log_layer;
mod memory;
pub mod perf;
mod redact;
mod report;

pub use disk_use::{DiskKind, DiskLocationUse, DiskTotals, DiskUse, scan_disk_use};
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
    BuildInfo, DpiAwareness, ErrorRecord, ReportAttachments, ReportInput, ReportSummary,
    SystemInfo, write_report, write_report_bundle, write_report_with_crashes,
    write_report_with_extras,
};
