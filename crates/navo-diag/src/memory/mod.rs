#[cfg(target_os = "linux")]
mod linux;

use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use crate::components::ComponentUse;
use crate::heap::HeapStats;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheSize {
    pub bytes: u64,
    pub pinned_bytes: u64,
    pub entries: u64,
}

impl CacheSize {
    #[must_use]
    pub fn unpinned_bytes(&self) -> u64 {
        self.bytes.saturating_sub(self.pinned_bytes)
    }
}

type Measure = Box<dyn Fn() -> CacheSize + Send + Sync>;

struct Registered {
    name: String,
    budget: u64,
    measure: Measure,
}

#[derive(Clone, Default)]
pub struct CacheRegistry {
    entries: Arc<Mutex<Vec<Registered>>>,
}

impl CacheRegistry {
    pub fn register(
        &self,
        name: &str,
        budget_bytes: u64,
        measure: impl Fn() -> CacheSize + Send + Sync + 'static,
    ) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Registered {
                name: name.to_owned(),
                budget: budget_bytes,
                measure: Box::new(measure),
            });
    }

    #[must_use]
    pub fn usage(&self) -> Vec<CacheUsage> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|cache| CacheUsage {
                name: cache.name.clone(),
                size: (cache.measure)(),
                budget: cache.budget,
            })
            .collect()
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
    pub size: CacheSize,
    pub budget: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryReport {
    pub process: Option<ProcessMemory>,
    pub probe_error: Option<String>,
    pub caches: Vec<CacheUsage>,
    pub tasks: Vec<(String, usize)>,
    pub heap: Option<HeapStats>,
    pub components: Vec<ComponentUse>,
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
        caches: caches.usage(),
        tasks: tasks.snapshot(),
        heap: None,
        components: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
