use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use navo_core::{FsError, FsErrorKind, Location};
use navo_fs::FsEntry;

use crate::command_center::flights::{Answer, Ask, Flights, Rules};
use crate::command_center::places::Places;
use crate::command_center::{Cancel, Hit, Target};

pub type LoadMetadata = Arc<dyn Fn(&Location) -> Result<FsEntry, FsError> + Send + Sync>;

#[derive(Clone)]
pub struct IndexMetadata {
    load: LoadMetadata,
    flights: Flights<Result<FsEntry, FsError>>,
}

impl IndexMetadata {
    #[must_use]
    pub fn new(load: LoadMetadata) -> Self {
        Self {
            load,
            flights: Flights::new(Rules {
                name: "index-metadata",
                wait: Duration::from_secs(2),
                is_down: |result| {
                    result.as_ref().is_err_and(|error| {
                        matches!(error.kind, FsErrorKind::NetworkLost | FsErrorKind::Timeout)
                    })
                },
                now: Arc::new(Instant::now),
            }),
        }
    }

    pub(super) fn load(
        &self,
        location: &Location,
        since: Instant,
        cancel: &Cancel,
    ) -> Option<FsEntry> {
        let ask = Ask {
            key: location,
            since,
            cancel: &|| cancel.is_cancelled(),
        };
        let (load, path) = (self.load.clone(), location.clone());
        match self.flights.get(&ask, move || load(&path)) {
            Answer::Ready(Ok(entry)) => Some(entry),
            _ => None,
        }
    }
}

/// Metadata of the index hits already shown, looked up by place in one hash step.
pub(super) struct Shown {
    opened: Instant,
    entries: Mutex<ShownEntries>,
}

/// The loaded entries and a place index into them; the first entry of a place wins.
#[derive(Default)]
struct ShownEntries {
    places: Places<usize>,
    entries: Vec<FsEntry>,
}

impl Default for Shown {
    fn default() -> Self {
        Self {
            opened: Instant::now(),
            entries: Mutex::new(ShownEntries::default()),
        }
    }
}

impl Shown {
    /// Returns the loaded metadata of the same place as `location`, if any.
    pub(super) fn get(&self, location: &Location) -> Option<FsEntry> {
        let shown = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let key = shown.places.key(location);
        let at = shown.places.find(&key)?;
        shown.entries.get(at).cloned()
    }

    pub(super) fn load(&self, loader: &IndexMetadata, hits: &[Hit], cancel: &Cancel) {
        for hit in hits {
            if cancel.is_cancelled() {
                return;
            }
            let Target::Open { location, .. } = &hit.target else {
                continue;
            };
            if hit.source != "index" || self.get(location).is_some() {
                continue;
            }
            if let Some(entry) = loader.load(location, self.opened, cancel) {
                let mut shown = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
                let key = shown.places.key(location);
                let at = shown.entries.len();
                shown.places.insert(key, at);
                shown.entries.push(entry);
            }
        }
    }
}
