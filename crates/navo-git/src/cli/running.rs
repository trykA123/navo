//! The process-wide registry of Git children and the pause a file operation takes over a folder.

use std::fmt;
#[cfg(windows)]
use std::path::Prefix;
use std::path::{Component, Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::GitError;

use super::process_watch::Cancel;

/// A far deadline used when `wait` cannot be added to the current instant.
const FALLBACK_WAIT: Duration = Duration::from_secs(365 * 24 * 60 * 60);

/// What a Git child does: a single command reads, a fetch or pull/push session writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Work {
    Read,
    Write,
}

/// A running Git child. Dropping it removes the registry entry and wakes pause waiters.
pub(crate) struct Ticket {
    id: u64,
    stop: Cancel,
    registry: &'static Running,
}

impl Ticket {
    /// A clone of the stop flag the spawner checks while the child runs.
    pub(crate) fn stop_flag(&self) -> Cancel {
        self.stop.clone()
    }
}

impl Drop for Ticket {
    /// Removes the child's entry and wakes any pause waiting for it.
    fn drop(&mut self) {
        let mut state = self.registry.lock();
        state.entries.retain(|entry| entry.id != self.id);
        self.registry.changed.notify_all();
    }
}

/// A live pause over a folder. Dropping it lets Git children start there again.
#[must_use]
pub struct GitPause {
    id: u64,
    registry: &'static Running,
}

impl fmt::Debug for GitPause {
    /// Prints the pause id.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitPause")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for GitPause {
    /// Removes the pause and wakes children waiting to register.
    fn drop(&mut self) {
        let mut state = self.registry.lock();
        state.pauses.retain(|(id, _)| *id != self.id);
        self.registry.changed.notify_all();
    }
}

/// The repository whose running write session outlasted the deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBusy {
    pub repo: PathBuf,
}

/// The process-wide registry state and the condition its waiters sleep on.
pub(crate) struct Running {
    state: Mutex<State>,
    changed: Condvar,
}

impl Running {
    /// An empty registry.
    pub(crate) const fn new() -> Self {
        Self {
            state: Mutex::new(State {
                next: 0,
                entries: Vec::new(),
                pauses: Vec::new(),
            }),
            changed: Condvar::new(),
        }
    }

    /// Locks the registry state, recovering a poisoned lock.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a child unless a live pause refuses it.
    fn register(&'static self, repo: &Path, work: Work) -> Result<Ticket, GitError> {
        let key = key(repo);
        let mut state = self.lock();
        if is_refused(&state, &key, work) {
            return Err(GitError::Paused);
        }
        let id = state.take_id();
        let stop = Cancel::default();
        state.entries.push(Entry {
            id,
            repo: repo.to_path_buf(),
            key,
            work,
            stop: stop.clone(),
        });
        Ok(Ticket {
            id,
            stop,
            registry: self,
        })
    }

    /// Waits for covered write sessions, installs the pause, and waits for its reads to stop.
    fn stop_under(&'static self, path: &Path, wait: Duration) -> Result<GitPause, GitBusy> {
        let key = key(path);
        let now = Instant::now();
        let deadline = now.checked_add(wait).unwrap_or(now + FALLBACK_WAIT);
        let state = self.lock();
        let mut state = self.await_covered_write(state, &key, deadline)?;
        let id = state.take_id();
        state.pauses.push((id, key.clone()));
        let stopped = flag_reads(&state, &key);
        let (mut state, busy) = self.await_flagged_reads(state, &stopped, deadline);
        if let Some(busy) = busy {
            state.pauses.retain(|(pause, _)| *pause != id);
            self.changed.notify_all();
            return Err(busy);
        }
        Ok(GitPause { id, registry: self })
    }

    /// Waits while a write session covered by `path` is still running.
    fn await_covered_write(
        &'static self,
        mut state: MutexGuard<'static, State>,
        path: &Key,
        deadline: Instant,
    ) -> Result<MutexGuard<'static, State>, GitBusy> {
        loop {
            let Some(entry) = state
                .entries
                .iter()
                .find(|entry| entry.work == Work::Write && covers(path, &entry.key))
            else {
                return Ok(state);
            };
            let repo = entry.repo.clone();
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(GitBusy { repo });
            }
            state = self.wait(state, remaining);
        }
    }

    /// Waits until every flagged read ticket has been dropped.
    fn await_flagged_reads(
        &'static self,
        mut state: MutexGuard<'static, State>,
        stopped: &[u64],
        deadline: Instant,
    ) -> (MutexGuard<'static, State>, Option<GitBusy>) {
        loop {
            let Some(entry) = state
                .entries
                .iter()
                .find(|entry| stopped.contains(&entry.id))
            else {
                return (state, None);
            };
            let repo = entry.repo.clone();
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return (state, Some(GitBusy { repo }));
            }
            state = self.wait(state, remaining);
        }
    }

    /// Waits for a registry change, recovering a poisoned lock.
    fn wait(
        &self,
        state: MutexGuard<'static, State>,
        timeout: Duration,
    ) -> MutexGuard<'static, State> {
        self.changed
            .wait_timeout(state, timeout)
            .unwrap_or_else(PoisonError::into_inner)
            .0
    }
}

/// Refuses a child whose repository overlaps a live pause.
fn is_refused(state: &State, key: &Key, work: Work) -> bool {
    state.pauses.iter().any(|(_, pause)| match work {
        Work::Read => overlaps(key, pause) && !is_session(state, key),
        Work::Write => overlaps(key, pause),
    })
}

/// Whether a write session with this exact repository is running.
fn is_session(state: &State, key: &Key) -> bool {
    state
        .entries
        .iter()
        .any(|entry| entry.work == Work::Write && entry.key == *key)
}

/// Stops every overlapping read outside a session repository and returns their ids.
fn flag_reads(state: &State, path: &Key) -> Vec<u64> {
    let mut stopped = Vec::new();
    for entry in &state.entries {
        if entry.work != Work::Read || !overlaps(&entry.key, path) {
            continue;
        }
        if is_session(state, &entry.key) {
            continue;
        }
        entry.stop.cancel();
        stopped.push(entry.id);
    }
    stopped
}

/// One path component as registry text, case-folded on Windows.
#[cfg(windows)]
fn component_text(component: Component<'_>) -> String {
    match component {
        Component::Prefix(prefix) => prefix_text(prefix.kind()),
        Component::RootDir => "\\".to_owned(),
        Component::CurDir => ".".to_owned(),
        Component::ParentDir => "..".to_owned(),
        Component::Normal(name) => name.to_string_lossy().to_lowercase(),
    }
}

/// One path component as registry text, compared exactly.
#[cfg(not(windows))]
fn component_text(component: Component<'_>) -> String {
    match component {
        Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().into_owned(),
        Component::RootDir => "/".to_owned(),
        Component::CurDir => ".".to_owned(),
        Component::ParentDir => "..".to_owned(),
        Component::Normal(name) => name.to_string_lossy().into_owned(),
    }
}

/// A Windows prefix without the verbatim marker, folded to lower case.
#[cfg(windows)]
fn prefix_text(prefix: Prefix<'_>) -> String {
    match prefix {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            format!("{}:", char::from(letter).to_ascii_lowercase())
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => format!(
            "\\\\{}\\{}",
            server.to_string_lossy().to_lowercase(),
            share.to_string_lossy().to_lowercase()
        ),
        Prefix::Verbatim(text) => text.to_string_lossy().to_lowercase(),
        Prefix::DeviceNS(text) => format!("\\\\.\\{}", text.to_string_lossy().to_lowercase()),
    }
}

/// The folder components a pause and a repository ticket compare.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Key(Vec<String>);

/// Builds the comparable key of a folder path.
///
/// Callers pass normalized paths: `..` is not resolved, which only causes a missed match.
fn key(path: &Path) -> Key {
    Key(path.components().map(component_text).collect())
}

/// Whether `inner` is `outer` or lies inside it.
fn covers(outer: &Key, inner: &Key) -> bool {
    inner.0.starts_with(&outer.0)
}

/// Whether either path is the other or lies inside it.
fn overlaps(a: &Key, b: &Key) -> bool {
    covers(a, b) || covers(b, a)
}

/// The registry entry of one running Git child.
struct Entry {
    id: u64,
    repo: PathBuf,
    key: Key,
    work: Work,
    stop: Cancel,
}

/// The registry contents, guarded by one mutex for registration and pausing.
struct State {
    next: u64,
    entries: Vec<Entry>,
    pauses: Vec<(u64, Key)>,
}

impl State {
    /// Returns the next unused id for a ticket or a pause.
    fn take_id(&mut self) -> u64 {
        self.next = self.next.wrapping_add(1);
        self.next
    }
}

/// The registry shared by every Git engine in the process.
static RUNNING: Running = Running::new();

/// Registers a Git child before it spawns, unless a pause refuses it.
pub(crate) fn register(repo: &Path, work: Work) -> Result<Ticket, GitError> {
    RUNNING.register(repo, work)
}

/// Stops Git children under `path`, waits up to `wait` in total, and pauses new ones there.
pub fn stop_under(path: &Path, wait: Duration) -> Result<GitPause, GitBusy> {
    RUNNING.stop_under(path, wait)
}

#[cfg(test)]
#[path = "running_tests.rs"]
mod tests;
