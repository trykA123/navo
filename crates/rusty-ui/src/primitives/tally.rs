use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use gpui_kit::{App, Global};
use rusty_diag::components::{Held, MemoryUse, Tally};

#[derive(Default)]
struct Count {
    bytes: AtomicU64,
    items: AtomicU64,
}

impl Count {
    fn add(&self, held: MemoryUse) {
        self.bytes.fetch_add(held.bytes, Relaxed);
        self.items.fetch_add(held.items, Relaxed);
    }

    fn sub(&self, held: MemoryUse) {
        self.bytes.fetch_sub(held.bytes, Relaxed);
        self.items.fetch_sub(held.items, Relaxed);
    }
}

/// One app's share of a process-wide [`Tally`].
#[derive(Clone, Default)]
pub struct AppTally(Arc<Count>);

impl AppTally {
    pub(crate) fn of(tally: &'static Tally, cx: &mut App) -> Self {
        cx.default_global::<Tallies>()
            .0
            .entry(key(tally))
            .or_default()
            .clone()
    }

    pub(crate) fn hold(&self, tally: &'static Tally, held: MemoryUse) -> AppHeld {
        self.0.add(held);
        AppHeld {
            _process: tally.hold(held),
            count: self.0.clone(),
            held,
        }
    }

    #[cfg(test)]
    pub(crate) fn read(&self) -> MemoryUse {
        MemoryUse {
            bytes: self.0.bytes.load(Relaxed),
            items: self.0.items.load(Relaxed),
        }
    }
}

#[derive(Default)]
struct Tallies(HashMap<usize, AppTally>);

fn key(tally: &'static Tally) -> usize {
    std::ptr::from_ref(tally) as usize
}

impl Global for Tallies {}

#[cfg(test)]
pub(crate) fn read(tally: &'static Tally, cx: &App) -> MemoryUse {
    cx.try_global::<Tallies>()
        .and_then(|tallies| tallies.0.get(&key(tally)))
        .map(AppTally::read)
        .unwrap_or_default()
}

pub(crate) struct AppHeld {
    _process: Held,
    count: Arc<Count>,
    held: MemoryUse,
}

impl Drop for AppHeld {
    fn drop(&mut self) {
        self.count.sub(self.held);
    }
}
