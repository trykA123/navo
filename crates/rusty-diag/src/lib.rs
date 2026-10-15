#![forbid(unsafe_code)]

mod memory;

pub use memory::{
    CacheHandle, CacheRegistry, CacheUsage, FakeProbe, MemoryProbe, MemoryReport, ProcessMemory,
    TaskCounters, TaskGuard, memory_report,
};
#[cfg(target_os = "linux")]
pub use memory::{ProcStatusProbe, parse_proc_status};
