use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, Once, PoisonError};

use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MemoryUse {
    pub bytes: u64,
    pub items: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    Use(MemoryUse),
    Busy,
    Gone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentUse {
    pub name: &'static str,
    pub bytes: u64,
    pub items: u64,
    pub busy: bool,
}

type Reporter = Box<dyn Fn() -> Reading + Send + Sync>;

#[derive(Default)]
pub struct ComponentRegistry {
    reporters: Mutex<Vec<(&'static str, Reporter)>>,
}

impl ComponentRegistry {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            reporters: Mutex::new(Vec::new()),
        }
    }

    pub fn register(
        &self,
        name: &'static str,
        reporter: impl Fn() -> Reading + Send + Sync + 'static,
    ) {
        self.reporters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((name, Box::new(reporter)));
    }

    #[must_use]
    pub fn usage(&self) -> Vec<ComponentUse> {
        let mut reporters = self
            .reporters
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut usage: Vec<ComponentUse> = Vec::new();
        reporters.retain(|(name, reporter)| {
            let reading = reporter();
            if reading != Reading::Gone {
                add(&mut usage, name, reading);
            }
            reading != Reading::Gone
        });
        drop(reporters);
        usage.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(b.name)));
        usage
    }
}

fn add(usage: &mut Vec<ComponentUse>, name: &'static str, reading: Reading) {
    let index = usage
        .iter()
        .position(|component| component.name == name)
        .unwrap_or_else(|| {
            usage.push(ComponentUse {
                name,
                bytes: 0,
                items: 0,
                busy: false,
            });
            usage.len() - 1
        });
    let component = &mut usage[index];
    match reading {
        Reading::Use(read) => {
            component.bytes = component.bytes.saturating_add(read.bytes);
            component.items = component.items.saturating_add(read.items);
        }
        Reading::Busy => component.busy = true,
        Reading::Gone => {}
    }
}

static GLOBAL: ComponentRegistry = ComponentRegistry::new();

pub fn register(name: &'static str, reporter: impl Fn() -> Reading + Send + Sync + 'static) {
    GLOBAL.register(name, reporter);
}

#[must_use]
pub fn usage() -> Vec<ComponentUse> {
    GLOBAL.usage()
}

pub struct Tally {
    name: &'static str,
    bytes: AtomicU64,
    items: AtomicU64,
    registered: Once,
}

impl Tally {
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            bytes: AtomicU64::new(0),
            items: AtomicU64::new(0),
            registered: Once::new(),
        }
    }

    pub fn hold(&'static self, held: MemoryUse) -> Held {
        self.registered
            .call_once(|| register(self.name, || Reading::Use(self.read())));
        self.bytes.fetch_add(held.bytes, Relaxed);
        self.items.fetch_add(held.items, Relaxed);
        Held { tally: self, held }
    }

    #[must_use]
    pub fn read(&self) -> MemoryUse {
        MemoryUse {
            bytes: self.bytes.load(Relaxed),
            items: self.items.load(Relaxed),
        }
    }
}

pub struct Held {
    tally: &'static Tally,
    held: MemoryUse,
}

impl Held {
    pub fn set(&mut self, held: MemoryUse) {
        let tally = self.tally;
        tally.bytes.fetch_add(held.bytes, Relaxed);
        tally.items.fetch_add(held.items, Relaxed);
        tally.bytes.fetch_sub(self.held.bytes, Relaxed);
        tally.items.fetch_sub(self.held.items, Relaxed);
        self.held = held;
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.set(MemoryUse::default());
    }
}

impl std::fmt::Debug for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Held")
            .field("tally", &self.tally.name)
            .field("held", &self.held)
            .finish()
    }
}

#[cfg(test)]
mod tests;
