use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

use rusty_core::{FsErrorKind, Location};
use rusty_fs::{CancelToken, EntryBatch, EntryKind, FsEntry, Provider};

use crate::table::{Flags, NO_PARENT, NameTable, TableError};

const WORKERS: usize = 4;
const MAX_DEPTH: u16 = 256;
const PROGRESS_EVERY: usize = 10_000;

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_entries: usize,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    pub entries: usize,
    pub skipped: usize,
    pub depth_limited: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Complete,
    Cancelled,
    EntryLimit,
}

#[derive(Debug)]
pub struct WalkResult {
    pub table: NameTable,
    pub progress: Progress,
    pub outcome: Outcome,
}

#[expect(
    clippy::too_many_arguments,
    reason = "signature follows the walker contract"
)]
pub fn walk(
    provider: &dyn Provider,
    root: &Location,
    limits: Limits,
    cancel: &CancelToken,
    on_progress: impl FnMut(&NameTable, Progress) + Send,
) -> Result<WalkResult, TableError> {
    let mut state = State {
        table: NameTable::new(),
        limits,
        on_progress,
        pending: VecDeque::new(),
        next: VecDeque::new(),
        active: 0,
        skipped: 0,
        depth_limited: 0,
        outcome: None,
        error: None,
        last_report: None,
    };
    state.seed(root, cancel)?;
    let shared = Shared {
        state: Mutex::new(state),
        wake: Condvar::new(),
    };
    thread::scope(|scope| {
        for _ in 0..WORKERS {
            scope.spawn(|| shared.run(provider, cancel));
        }
    });
    let mut state = shared
        .state
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    state.report();
    if let Some(error) = state.error {
        return Err(error);
    }
    Ok(WalkResult {
        progress: state.progress(),
        table: state.table,
        outcome: state.outcome.unwrap_or(Outcome::Complete),
    })
}

struct Folder {
    location: Location,
    id: u32,
    depth: u16,
}

struct State<F> {
    table: NameTable,
    limits: Limits,
    on_progress: F,
    pending: VecDeque<Folder>,
    next: VecDeque<Folder>,
    active: usize,
    skipped: usize,
    depth_limited: usize,
    outcome: Option<Outcome>,
    error: Option<TableError>,
    last_report: Option<Progress>,
}

impl<F: FnMut(&NameTable, Progress)> State<F> {
    fn seed(&mut self, root: &Location, cancel: &CancelToken) -> Result<(), TableError> {
        if self.stopped(cancel) {
            return Ok(());
        }
        let name = root.name().map(|name| name.to_text()).unwrap_or_default();
        if self.excluded(root, &name) {
            self.outcome = Some(Outcome::Complete);
            return Ok(());
        }
        let id = self
            .table
            .push(NO_PARENT, &root_name(root), Flags::FOLDER)?;
        self.pending.push_back(Folder {
            location: root.clone(),
            id,
            depth: 0,
        });
        Ok(())
    }

    fn stopped(&mut self, cancel: &CancelToken) -> bool {
        if self.outcome.is_some() || self.error.is_some() {
            return true;
        }
        self.outcome = if cancel.is_cancelled() {
            Some(Outcome::Cancelled)
        } else if self.table.len() >= self.limits.max_entries {
            Some(Outcome::EntryLimit)
        } else {
            None
        };
        self.outcome.is_some()
    }

    fn take(
        &mut self,
        folder: &Folder,
        batch: EntryBatch,
        cancel: &CancelToken,
    ) -> ControlFlow<()> {
        for entry in batch.entries {
            if self.stopped(cancel) {
                return ControlFlow::Break(());
            }
            if let Err(error) = self.record(folder, entry) {
                self.error = Some(error);
                return ControlFlow::Break(());
            }
        }
        if self.stopped(cancel) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn record(&mut self, folder: &Folder, entry: FsEntry) -> Result<(), TableError> {
        let Ok(location) = folder.location.join(&entry.name) else {
            self.skipped += usize::from(entry.kind == EntryKind::Folder);
            return Ok(());
        };
        let name = entry.name.to_text();
        if self.excluded(&location, &name) {
            return Ok(());
        }
        let id = self.table.push(folder.id, &name, flags(&entry))?;
        if entry.kind == EntryKind::Folder {
            let depth = folder.depth + 1;
            if depth == MAX_DEPTH {
                self.depth_limited += 1;
            } else {
                self.next.push_back(Folder {
                    location,
                    id,
                    depth,
                });
            }
        }
        if self.table.len().is_multiple_of(PROGRESS_EVERY) {
            self.report();
        }
        Ok(())
    }

    fn excluded(&self, location: &Location, name: &str) -> bool {
        let path = location.path().to_text();
        self.limits
            .exclude
            .iter()
            .any(|exclude| exclude == name || exclude == &path || exclude == location.as_str())
    }

    fn progress(&self) -> Progress {
        Progress {
            entries: self.table.len(),
            skipped: self.skipped,
            depth_limited: self.depth_limited,
        }
    }

    fn report(&mut self) {
        let progress = self.progress();
        if self.last_report != Some(progress) {
            (self.on_progress)(&self.table, progress);
            self.last_report = Some(progress);
        }
    }
}

struct Shared<F> {
    state: Mutex<State<F>>,
    wake: Condvar,
}

impl<F: FnMut(&NameTable, Progress) + Send> Shared<F> {
    fn run(&self, provider: &dyn Provider, cancel: &CancelToken) {
        while let Some(folder) = self.next_folder(cancel) {
            let mut sink = |batch| self.lock().take(&folder, batch, cancel);
            let result = provider.list(&folder.location, &mut sink, cancel);
            let mut state = self.lock();
            state.active -= 1;
            match result {
                Err(error) if error.kind == FsErrorKind::Cancelled => {
                    state.outcome = Some(Outcome::Cancelled);
                }
                Err(_) => state.skipped += 1,
                Ok(()) => {}
            }
            self.wake.notify_all();
        }
    }

    fn next_folder(&self, cancel: &CancelToken) -> Option<Folder> {
        let mut state = self.lock();
        loop {
            if state.stopped(cancel) {
                self.wake.notify_all();
                return None;
            }
            if let Some(folder) = state.pending.pop_front() {
                state.active += 1;
                return Some(folder);
            }
            if state.active == 0 && state.next.is_empty() {
                state.outcome = Some(Outcome::Complete);
                self.wake.notify_all();
                return None;
            }
            if state.active == 0 {
                state.pending = std::mem::take(&mut state.next);
                continue;
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn lock(&self) -> MutexGuard<'_, State<F>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn flags(entry: &FsEntry) -> Flags {
    let mut flags = Flags::NONE;
    if entry.kind == EntryKind::Folder || entry.attributes.link_to_folder {
        flags = flags | Flags::FOLDER;
    }
    if entry.kind == EntryKind::Link {
        flags = flags | Flags::REPARSE;
    }
    if entry.attributes.hidden {
        flags = flags | Flags::HIDDEN;
    }
    if entry.attributes.system {
        flags = flags | Flags::SYSTEM;
    }
    flags
}

fn root_name(root: &Location) -> String {
    let text = root.path().to_text();
    if root.is_local() && root.root().path().to_text().ends_with('\\') {
        text.trim_end_matches(['\\', '/']).to_owned()
    } else {
        text.trim_end_matches('/').to_owned()
    }
}

#[cfg(test)]
mod tests;
