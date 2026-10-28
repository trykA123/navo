mod walk;

use std::ops::ControlFlow;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusty_core::find::{Attribute, Date, FileHits, FindQuery, ModifiedFilter, SizeFilter};
use rusty_core::{FsError, Location, ProviderPath};

use crate::{
    CancelToken, FsEntry, JobKind, JobSlot, LocalFs, Pools, Priority, Provider, os_path, volume_of,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundFile {
    pub relative: ProviderPath,
    pub entry: FsEntry,
    pub hits: FileHits,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindProgress {
    pub scanned: u64,
    pub hits: u64,
    pub matched: u64,
    pub skipped: u64,
    pub current: Option<Location>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundBatch {
    pub files: Vec<FoundFile>,
    pub progress: FindProgress,
}

#[derive(Debug, thiserror::Error)]
pub enum FindError {
    #[error("Search inside files works on local and network drives.")]
    Unsupported,
    #[error("The search was cancelled.")]
    Cancelled,
    #[error("Invalid search: {0}")]
    Invalid(String),
    #[error("{}", .0.message())]
    Filesystem(FsError),
    #[error("The search worker stopped.")]
    WorkerStopped,
}

pub fn find_files(
    base: &Location,
    query: &FindQuery,
    cancel: &CancelToken,
    sink: impl FnMut(FoundBatch) -> ControlFlow<()>,
) -> Result<FindProgress, FindError> {
    let root = scope(base, query)?;
    if cancel.is_cancelled() {
        return Err(FindError::Cancelled);
    }
    let compiled = rusty_core::find::Compiled::new(query).map_err(FindError::Invalid)?;
    let queue = Pools::shared().queue(JobKind::Find);
    let owner = queue.owner();
    let slot = JobSlot {
        volume: volume_of(&root),
        priority: Priority::Idle,
        owner: owner.id(),
    };
    let (send, receive) = mpsc::sync_channel(1);
    let signal = cancel.clone();
    let query = query.clone();
    queue.submit(
        slot,
        Box::new(move |_| {
            let result = walk::Walk::new(&query, &signal, &mut |batch| {
                if send.send(Event::Batch(batch)).is_ok() {
                    ControlFlow::Continue(())
                } else {
                    ControlFlow::Break(())
                }
            })
            .run(&root, &compiled);
            let _ = send.send(Event::Done(result));
        }),
    );
    receive_batches(receive, cancel, sink)
}

pub fn walk_files(
    base: &Location,
    query: &FindQuery,
    cancel: &CancelToken,
    mut sink: impl FnMut(FoundBatch) -> ControlFlow<()>,
) -> Result<FindProgress, FindError> {
    let root = scope(base, query)?;
    if cancel.is_cancelled() {
        return Err(FindError::Cancelled);
    }
    let compiled = rusty_core::find::Compiled::new(query).map_err(FindError::Invalid)?;
    walk::Walk::new(query, cancel, &mut |batch| {
        sink(rebase_batch(batch, base, &root))
    })
    .run(&root, &compiled)
}

fn rebase_batch(mut batch: FoundBatch, base: &Location, root: &Location) -> FoundBatch {
    if base == root {
        return batch;
    }
    let base = base.as_str().trim_end_matches(['\\', '/']);
    let prefix = root
        .as_str()
        .strip_prefix(base)
        .and_then(|rest| rest.strip_prefix(['\\', '/']))
        .unwrap_or_else(|| root.as_str());
    for file in &mut batch.files {
        file.relative = ProviderPath::from_text(&format!(
            "{}{}{}",
            prefix.trim_end_matches(['\\', '/']),
            std::path::MAIN_SEPARATOR,
            file.relative.to_text()
        ));
    }
    batch
}

fn receive_batches(
    receive: Receiver<Event>,
    cancel: &CancelToken,
    mut sink: impl FnMut(FoundBatch) -> ControlFlow<()>,
) -> Result<FindProgress, FindError> {
    loop {
        if cancel.is_cancelled() {
            return Err(FindError::Cancelled);
        }
        match receive.recv_timeout(Duration::from_millis(25)) {
            Ok(Event::Batch(batch)) => {
                if sink(batch).is_break() {
                    cancel.cancel();
                    return Err(FindError::Cancelled);
                }
            }
            Ok(Event::Done(result)) => return result,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(FindError::WorkerStopped),
        }
    }
}

pub(super) enum Event {
    Batch(FoundBatch),
    Done(Result<FindProgress, FindError>),
}

fn scope(base: &Location, query: &FindQuery) -> Result<Location, FindError> {
    if !base.is_local() {
        return Err(FindError::Unsupported);
    }
    let Some(within) = &query.within else {
        return Ok(base.clone());
    };
    let location =
        Location::new(within.clone()).map_err(|error| FindError::Invalid(error.to_string()))?;
    if !location.is_local() {
        return Err(FindError::Unsupported);
    }
    let path = os_path(&location).map_err(FindError::Filesystem)?;
    if path.is_absolute() {
        return Ok(location);
    }
    within
        .split(std::path::is_separator)
        .filter(|part| !part.is_empty() && *part != ".")
        .try_fold(base.clone(), |folder, part| {
            if part == ".." {
                return folder
                    .parent()
                    .ok_or_else(|| FindError::Invalid(within.clone()));
            }
            folder
                .join(&ProviderPath::from_text(part))
                .map_err(|error| FindError::Invalid(error.to_string()))
        })
}

struct Filters<'a> {
    query: &'a FindQuery,
    now: i64,
}

impl<'a> Filters<'a> {
    fn new(query: &'a FindQuery) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())
            .unwrap_or_default();
        Self { query, now }
    }

    fn folder(&self, entry: &FsEntry) -> bool {
        entry.name.as_str() != Some(".git")
            && (!entry.attributes.hidden || self.includes(Attribute::Hidden))
            && (!entry.attributes.system || self.includes(Attribute::System))
    }

    fn includes(&self, attribute: Attribute) -> bool {
        self.query
            .attrs
            .iter()
            .any(|filter| filter.attribute == attribute && !filter.excluded)
    }

    fn file(&self, location: Option<&Location>, entry: &FsEntry) -> bool {
        let name = entry.name.to_text();
        if !self.query.ext.is_empty()
            && !name.rsplit_once('.').is_some_and(|(_, ext)| {
                self.query
                    .ext
                    .iter()
                    .any(|allowed| ext.eq_ignore_ascii_case(allowed))
            })
        {
            return false;
        }
        if self
            .query
            .size
            .is_some_and(|filter| !matches_size(filter, entry.size))
            || self
                .query
                .modified
                .is_some_and(|filter| !self.modified(filter, entry.modified_ms))
        {
            return false;
        }
        self.query.attrs.iter().all(|filter| {
            let value = match filter.attribute {
                Attribute::Hidden => entry.attributes.hidden,
                Attribute::Readonly => entry.attributes.readonly,
                Attribute::System => entry.attributes.system,
                Attribute::Archive => {
                    match location.map(|location| LocalFs::new().details(location)) {
                        Some(Ok(details)) => details.attributes.archive,
                        Some(Err(_)) => return false,
                        None => false,
                    }
                }
            };
            value != filter.excluded
        })
    }

    fn modified(&self, filter: ModifiedFilter, stamp: Option<i64>) -> bool {
        const DAY: i64 = 86_400_000;
        let Some(stamp) = stamp else {
            return false;
        };
        match filter {
            ModifiedFilter::YoungerThanDays(days) => {
                stamp > self.now.saturating_sub(i64::from(days) * DAY)
            }
            ModifiedFilter::OlderThanDays(days) => {
                stamp < self.now.saturating_sub(i64::from(days) * DAY)
            }
            ModifiedFilter::Before(date) => stamp < date_days(date) * DAY,
            ModifiedFilter::After(date) => stamp > date_days(date) * DAY,
            ModifiedFilter::Today => stamp.div_euclid(DAY) == self.now.div_euclid(DAY),
        }
    }
}

fn matches_size(filter: SizeFilter, size: u64) -> bool {
    match filter {
        SizeFilter::LessThan(limit) => size < limit,
        SizeFilter::GreaterThan(limit) => size > limit,
        SizeFilter::Range(start, end) => (start..=end).contains(&size),
    }
}

fn date_days(date: Date) -> i64 {
    let year = i64::from(date.year) - i64::from(date.month <= 2);
    let era = year.div_euclid(400);
    let year = year - era * 400;
    let month = i64::from(date.month) + if date.month > 2 { -3 } else { 9 };
    let day = (153 * month + 2) / 5 + i64::from(date.day) - 1;
    era * 146097 + year * 365 + year / 4 - year / 100 + day - 719468
}

#[cfg(test)]
mod tests;
