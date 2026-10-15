#[cfg(target_os = "linux")]
mod linux;

use std::collections::BTreeMap;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

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
        self.0
            .as_ref()
            .copied()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))
    }
}

type CacheEntry = (String, u64, Weak<AtomicU64>);

#[derive(Clone, Default)]
pub struct CacheRegistry {
    entries: Arc<Mutex<Vec<CacheEntry>>>,
}

impl CacheRegistry {
    pub fn register(&self, name: &str, budget_bytes: u64) -> CacheHandle {
        let bytes = Arc::new(AtomicU64::new(0));
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|(_, _, bytes)| bytes.strong_count() > 0);
        entries.push((name.to_owned(), budget_bytes, Arc::downgrade(&bytes)));
        CacheHandle { bytes }
    }

    fn snapshot(&self) -> Vec<CacheUsage> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let mut caches = Vec::with_capacity(entries.len());
        entries.retain(|(name, budget, bytes)| {
            let Some(bytes) = bytes.upgrade() else {
                return false;
            };
            caches.push(CacheUsage {
                name: name.clone(),
                bytes: bytes.load(Ordering::Relaxed),
                budget: *budget,
            });
            true
        });
        caches
    }
}

#[derive(Clone)]
pub struct CacheHandle {
    bytes: Arc<AtomicU64>,
}

impl CacheHandle {
    pub fn set(&self, bytes: u64) {
        self.bytes.store(bytes, Ordering::Relaxed);
    }

    pub fn add(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn sub(&self, bytes: u64) {
        let _ = self
            .bytes
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(bytes))
            });
    }
}

#[derive(Clone, Default)]
pub struct TaskCounters {
    counts: Arc<Mutex<BTreeMap<String, usize>>>,
}

impl TaskCounters {
    pub fn start(&self, owner: &str) -> TaskGuard {
        let owner = owner.to_owned();
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        *counts.entry(owner.clone()).or_default() += 1;
        TaskGuard {
            counts: Arc::clone(&self.counts),
            owner,
        }
    }

    fn snapshot(&self) -> Vec<(String, usize)> {
        let counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        counts
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(owner, count)| (owner.clone(), *count))
            .collect()
    }
}

pub struct TaskGuard {
    counts: Arc<Mutex<BTreeMap<String, usize>>>,
    owner: String,
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = counts.get_mut(&self.owner) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.remove(&self.owner);
            }
        }
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
    let (process, probe_error) = match probe.sample() {
        Ok(process) => (Some(process), None),
        Err(error) => (None, Some(error.to_string())),
    };
    MemoryReport {
        process,
        probe_error,
        caches: caches.snapshot(),
        tasks: tasks.snapshot(),
    }
}

#[cfg(test)]
mod tests;
