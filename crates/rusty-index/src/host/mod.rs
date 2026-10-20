mod scan;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, PoisonError, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusty_core::{Location, ProviderPath};
use rusty_fs::{
    CancelToken, IoQueue, JobSlot, OwnerGuard, Priority, Provider, WatchEvent, volume_of,
};

use crate::file::{self, FileError, Header};
use crate::table::NameTable;
use crate::walker::{Limits, Progress};

const POLL: Duration = Duration::from_millis(100);
type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

#[derive(Clone)]
pub struct HostOptions {
    pub provider: Arc<dyn Provider>,
    pub queue: Arc<IoQueue>,
    pub root: Location,
    pub snapshot: PathBuf,
    pub limits: Limits,
}

#[derive(Debug, Clone, Default)]
pub struct HostStatus {
    pub entries: usize,
    pub progress: Option<Progress>,
    pub errors: Vec<String>,
}

enum Message {
    Watch(WatchEvent),
    Stop,
    #[cfg(test)]
    Barrier(Sender<()>),
}

pub struct Host {
    table: Arc<RwLock<NameTable>>,
    statuses: Receiver<HostStatus>,
    sender: Sender<Message>,
    cancel: CancelToken,
    owner: Option<OwnerGuard>,
    thread: Option<JoinHandle<()>>,
}

impl Host {
    pub fn start(options: HostOptions) -> std::io::Result<Self> {
        Self::start_with_clock(options, Arc::new(Instant::now))
    }

    fn start_with_clock(options: HostOptions, clock: Clock) -> std::io::Result<Self> {
        let (sender, events) = mpsc::channel();
        let (updates, statuses) = mpsc::channel();
        let table = Arc::new(RwLock::new(NameTable::new()));
        let cancel = CancelToken::new();
        let owner = options.queue.owner();
        let worker = Worker {
            status: HostStatus::default(),
            slot: JobSlot {
                volume: volume_of(&options.root),
                priority: Priority::Idle,
                owner: owner.id(),
            },
            options,
            clock,
            events,
            updates,
            table: Arc::clone(&table),
            cancel: cancel.clone(),
            dirty: None,
        };
        let worker_sender = sender.clone();
        let thread = thread::Builder::new()
            .name("walk-index-host".into())
            .spawn(move || worker.run(worker_sender))?;
        Ok(Self {
            table,
            statuses,
            sender,
            cancel,
            owner: Some(owner),
            thread: Some(thread),
        })
    }

    pub fn table(&self) -> Arc<RwLock<NameTable>> {
        Arc::clone(&self.table)
    }

    pub fn try_status(&self) -> Option<HostStatus> {
        self.statuses.try_iter().last()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.cancel.cancel();
        drop(self.owner.take());
        let _ = self.sender.send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Worker {
    options: HostOptions,
    clock: Clock,
    table: Arc<RwLock<NameTable>>,
    events: Receiver<Message>,
    updates: Sender<HostStatus>,
    cancel: CancelToken,
    slot: JobSlot,
    status: HostStatus,
    dirty: Option<Instant>,
}

impl Worker {
    fn run(mut self, sender: Sender<Message>) {
        self.load();
        let watch = self.options.provider.watch_tree(
            &self.options.root,
            Box::new(move |event| {
                let _ = sender.send(Message::Watch(event));
            }),
        );
        if let Err(error) = &watch {
            self.error(format!("watching root: {}", error.message()));
        }
        self.scan();
        while !self.cancel.is_cancelled() {
            match self.events.recv_timeout(POLL) {
                Ok(Message::Stop) => break,
                Ok(Message::Watch(event)) => self.update(event),
                #[cfg(test)]
                Ok(Message::Barrier(reply)) => {
                    self.save_due();
                    let _ = reply.send(());
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            self.save_due();
        }
        drop(watch);
        while let Ok(message) = self.events.try_recv() {
            if let Message::Watch(event) = message {
                self.update(event);
            }
        }
        self.save();
    }

    fn load(&mut self) {
        match file::load(&self.options.snapshot) {
            Ok((header, mut table)) if header.root == self.options.root.as_str() => {
                let excluded: Vec<_> = table
                    .iter()
                    .filter_map(|(id, name)| {
                        let location = self.location(&table, id);
                        location
                            .is_none_or(|location| self.excluded(&location, name))
                            .then_some(id)
                    })
                    .collect();
                for id in excluded {
                    table.remove(id);
                }
                *self.table.write().unwrap_or_else(PoisonError::into_inner) = table;
            }
            Err(FileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => self.error("loading index: snapshot root differs".into()),
            Err(error) => self.error(format!("loading index: {error}")),
        }
        self.publish();
    }

    fn windows(&self) -> bool {
        self.options.root.is_local() && self.slot.volume.as_str() != "/"
    }

    fn resolve(&self, path: &ProviderPath) -> Option<Location> {
        let mut location = self.options.root.clone();
        let name = location.name().map(|name| name.to_text());
        if self.excluded(&location, name.as_deref().unwrap_or_default()) {
            return None;
        }
        let text = path.to_text();
        let parts: Vec<_> = text
            .split(|ch| ch == '/' || (self.windows() && ch == '\\'))
            .collect();
        if parts.len() > 256 {
            return None;
        }
        for part in parts {
            location = location.join(&ProviderPath::from_text(part)).ok()?;
            if self.excluded(&location, part) {
                return None;
            }
        }
        Some(location)
    }

    fn excluded(&self, location: &Location, name: &str) -> bool {
        let path = location.path().to_text();
        let paths = [name, path.as_str(), location.as_str()];
        let exclude = &self.options.limits.exclude;
        exclude.iter().any(|name| paths.contains(&name.as_str()))
    }

    fn location(&self, table: &NameTable, id: u32) -> Option<Location> {
        if id == 0 {
            return Some(self.options.root.clone());
        }
        let path = table.path_with(id, if self.windows() { '\\' } else { '/' })?;
        let provider = self.options.root.provider().clone();
        Location::with_provider(provider, ProviderPath::from_text(&path)).ok()
    }

    fn find(&self, table: &NameTable, location: &Location) -> Option<u32> {
        table
            .iter()
            .find_map(|(id, _)| self.location(table, id)?.same_place(location).then_some(id))
    }

    fn save_due(&mut self) {
        let Some(at) = self.dirty else {
            return;
        };
        if (self.clock)().saturating_duration_since(at) >= Duration::from_secs(30) {
            self.save();
        }
    }

    fn save(&mut self) {
        if self.dirty.is_none() {
            return;
        }
        let result: Result<(), FileError> = (|| {
            if let Some(parent) = self.options.snapshot.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let header = Header {
                root: self.options.root.as_str().to_owned(),
                volume_serial: 0,
                journal_id: 0,
                next_usn: 0,
                built_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|time| time.as_secs() as i64)
                    .unwrap_or_default(),
            };
            let table = self.table.read().unwrap_or_else(PoisonError::into_inner);
            file::save(&self.options.snapshot, &header, &table)
        })();
        self.dirty = result.is_err().then(|| (self.clock)());
        if let Err(error) = result {
            self.error(format!("saving index: {error}"));
            self.publish();
        }
    }

    fn error(&mut self, error: String) {
        if !self.status.errors.contains(&error) {
            self.status.errors.push(error);
        }
    }

    fn publish(&mut self) {
        let table = self.table.read().unwrap_or_else(PoisonError::into_inner);
        self.status.entries = table.len();
        let _ = self.updates.send(self.status.clone());
    }
}

#[cfg(test)]
mod tests;
