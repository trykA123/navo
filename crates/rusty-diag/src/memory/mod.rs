#[cfg(target_os = "linux")]
mod linux;

use std::collections::BTreeMap;
use std::io;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
pub use linux::{ProcStatusProbe, parse_proc_status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessMemory {
    pub resident_bytes: u64,
    pub private_bytes: u64,
}

pub trait MemoryProbe: Send + Sync {
    fn sample(&self) -> io::Result<ProcessMemory>;
}

pub struct FakeProbe(pub io::Result<ProcessMemory>);

impl MemoryProbe for FakeProbe {
    fn sample(&self) -> io::Result<ProcessMemory> {
        todo!()
    }
}

type CacheEntry = (String, u64, Arc<AtomicU64>);

#[derive(Clone, Default)]
pub struct CacheRegistry {
    entries: Arc<Mutex<Vec<CacheEntry>>>,
}

impl CacheRegistry {
    pub fn register(&self, name: &str, budget_bytes: u64) -> CacheHandle {
        todo!()
    }
}

#[derive(Clone)]
pub struct CacheHandle {
    bytes: Arc<AtomicU64>,
}

impl CacheHandle {
    pub fn set(&self, bytes: u64) {
        todo!()
    }

    pub fn add(&self, bytes: u64) {
        todo!()
    }

    pub fn sub(&self, bytes: u64) {
        todo!()
    }
}

#[derive(Clone, Default)]
pub struct TaskCounters {
    counts: Arc<Mutex<BTreeMap<String, usize>>>,
}

impl TaskCounters {
    pub fn start(&self, owner: &str) -> TaskGuard {
        todo!()
    }
}

pub struct TaskGuard {
    counts: Arc<Mutex<BTreeMap<String, usize>>>,
    owner: String,
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheUsage {
    pub name: String,
    pub bytes: u64,
    pub budget: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryReport {
    pub process: Option<ProcessMemory>,
    pub probe_error: Option<String>,
    pub caches: Vec<CacheUsage>,
    pub tasks: Vec<(String, usize)>,
}

pub fn memory_report(
    probe: &dyn MemoryProbe,
    caches: &CacheRegistry,
    tasks: &TaskCounters,
) -> MemoryReport {
    todo!()
}

#[cfg(test)]
mod tests;
