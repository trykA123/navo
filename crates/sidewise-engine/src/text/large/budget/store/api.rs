use std::ops::Range;
use std::sync::{Arc, PoisonError};

use crate::model::Reason;

use super::super::super::LargeDiff;
use super::super::{Ceiling, Rediff, rediff};

impl LargeDiff {
    /// Replaces one emitted section's hunks; the store charges them until eviction is acknowledged.
    pub fn hunks_for(&self, section: Range<u64>) -> Result<Rediff, Reason> {
        if self.cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
        let (context, pair, watchdog) = {
            let store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
            let store = store.as_ref().ok_or(Reason::Unsupported)?;
            let pair = store.find(&section).ok_or(Reason::Unsupported)?;
            (Arc::clone(&store.context), pair, store.watchdog)
        };
        let (rediff, charge) = rediff(
            &context,
            &pair,
            &self.ledger,
            Ceiling {
                cancel: &self.cancel,
                watchdog,
            },
        )?;
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        let store = store.as_mut().ok_or(Reason::Unsupported)?;
        if !Arc::ptr_eq(&context, &store.context) {
            return Err(Reason::ChangedOnDisk);
        }
        store.restore(&section, &rediff, charge)?;
        Ok(rediff)
    }

    #[must_use]
    pub fn memory_used(&self) -> u64 {
        self.ledger.used()
    }

    #[must_use]
    pub fn peak_memory(&self) -> u64 {
        self.ledger.peak()
    }

    /// Releases streamed eviction charges up to the highest applied event sequence.
    /// Call only after applying every `Paged` event with `seq <= upto`.
    pub fn acknowledge_evictions(&self, upto: u64) {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(store) = store.as_mut() {
            store.acknowledge(upto);
        }
    }
}
