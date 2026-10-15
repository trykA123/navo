use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use rusty_core::{
    AppState, Command, CommandError, CursorMove, Effect, Event, FsError, Listing, Location,
    ProviderPath, TabId, apply, apply_event, list_missing,
};
use rusty_fs::{
    EntryKind, FolderSnapshot, FsEntry, IoQueue, JobSlot, OwnerGuard, Priority, Provider,
    folder_size, load_folder, volume_of,
};

pub type Waker = Arc<dyn Fn() + Send + Sync>;

struct Loaded {
    tab: TabId,
    location: Location,
    request: u64,
    result: Result<FolderSnapshot, FsError>,
}

struct Measured {
    tab: TabId,
    folder: Location,
    name: String,
    bytes: u64,
}

enum Message {
    Loaded(Loaded),
    Measured(Measured),
}

pub struct Browser {
    state: AppState,
    provider: Arc<dyn Provider>,
    queue: IoQueue,
    owner: OwnerGuard,
    sizes: IoQueue,
    sizes_owner: OwnerGuard,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    wake: Waker,
    details: HashMap<TabId, Rc<[FsEntry]>>,
    errors: HashMap<TabId, FsError>,
    focus_after_load: HashMap<TabId, String>,
    in_flight: usize,
    save_requested: bool,
}

impl Browser {
    pub fn new(state: AppState, provider: Arc<dyn Provider>, wake: Waker) -> Self {
        let queue = IoQueue::new();
        let owner = queue.owner();
        let sizes = IoQueue::new();
        let sizes_owner = sizes.owner();
        let (sender, receiver) = mpsc::channel();
        Self {
            state,
            provider,
            queue,
            owner,
            sizes,
            sizes_owner,
            sender,
            receiver,
            wake,
            details: HashMap::new(),
            errors: HashMap::new(),
            focus_after_load: HashMap::new(),
            in_flight: 0,
            save_requested: false,
        }
    }

    pub fn start(&mut self) {
        let effects = list_missing(&mut self.state);
        self.run(effects);
    }

    pub fn command(&mut self, command: &Command) -> Result<(), CommandError> {
        let effects = apply(&mut self.state, command)?;
        self.run(effects);
        Ok(())
    }

    pub fn event(&mut self, event: Event) {
        let effects = apply_event(&mut self.state, event);
        self.run(effects);
    }

    pub fn open_row(&mut self, row: u64) -> bool {
        let tab = self.state.active_tab();
        let Some(entry) = tab.listing().and_then(|l| l.get(row)) else {
            return false;
        };
        let is_link = usize::try_from(row)
            .ok()
            .and_then(|index| self.details.get(&tab.id())?.get(index).cloned())
            .is_some_and(|detail| detail.kind == EntryKind::Link);
        if !entry.is_folder && !is_link {
            return false;
        }
        let Ok(location) = tab.location().join(&ProviderPath::from_text(&entry.name)) else {
            return false;
        };
        self.command(&Command::PaneNavigate { location }).is_ok()
    }

    pub fn go_up(&mut self) -> bool {
        let tab = self.state.active_tab();
        let Some(parent) = tab.location().parent() else {
            return false;
        };
        let child = tab
            .location()
            .path()
            .to_text()
            .rsplit(['/', '\\'])
            .find(|segment| !segment.is_empty())
            .map(str::to_owned);
        let id = tab.id();
        if self
            .command(&Command::PaneNavigate { location: parent })
            .is_err()
        {
            return false;
        }
        if let Some(child) = child {
            self.focus_after_load.insert(id, child);
        }
        true
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(message) = self.receiver.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            changed |= match message {
                Message::Loaded(loaded) => self.accept(loaded),
                Message::Measured(measured) => self.accept_size(&measured),
            };
        }
        changed
    }

    #[must_use]
    pub fn loading(&self) -> bool {
        self.in_flight > 0
    }

    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    #[must_use]
    pub fn details(&self, tab: TabId) -> Rc<[FsEntry]> {
        self.details
            .get(&tab)
            .filter(|_| self.is_loaded(tab))
            .cloned()
            .unwrap_or_else(|| Rc::from([]))
    }

    #[must_use]
    pub fn error(&self, tab: TabId) -> Option<&FsError> {
        self.errors.get(&tab).filter(|_| self.is_loaded(tab))
    }

    fn is_loaded(&self, tab: TabId) -> bool {
        self.state.tab(tab).is_some_and(|t| t.listing().is_some())
    }

    pub fn take_save_request(&mut self) -> bool {
        std::mem::take(&mut self.save_requested)
    }

    fn run(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::ListFolder {
                    tab,
                    location,
                    request,
                } => self.list(tab, location, request),
                Effect::SaveSession => self.save_requested = true,
                Effect::MeasureFolder {
                    tab,
                    location,
                    name,
                } => self.measure(tab, location, name),
            }
        }
    }

    fn list(&mut self, tab: TabId, location: Location, request: u64) {
        let (provider, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let slot = JobSlot {
            volume: volume_of(&location),
            priority: Priority::Visible,
            owner: self.owner.id(),
        };
        self.in_flight += 1;
        self.queue.submit(
            slot,
            Box::new(move |cancel| {
                let result = load_folder(provider.as_ref(), &location, cancel);
                let _ = sender.send(Message::Loaded(Loaded {
                    tab,
                    location,
                    request,
                    result,
                }));
                wake();
            }),
        );
    }

    fn measure(&mut self, tab: TabId, folder: Location, name: String) {
        let Ok(target) = folder.join(&ProviderPath::from_text(&name)) else {
            return;
        };
        let (provider, sender, wake) = (
            Arc::clone(&self.provider),
            self.sender.clone(),
            Arc::clone(&self.wake),
        );
        let slot = JobSlot {
            volume: volume_of(&target),
            priority: Priority::Idle,
            owner: self.sizes_owner.id(),
        };
        self.in_flight += 1;
        self.sizes.submit(
            slot,
            Box::new(move |cancel| {
                let bytes = folder_size(provider.as_ref(), &target, cancel).map_or(0, |s| s.bytes);
                let _ = sender.send(Message::Measured(Measured {
                    tab,
                    folder,
                    name,
                    bytes,
                }));
                wake();
            }),
        );
    }

    fn accept_size(&mut self, measured: &Measured) -> bool {
        let Some(tab) = self.state.tab(measured.tab) else {
            return false;
        };
        if tab.location() != &measured.folder {
            return false;
        }
        let Some(row) = tab
            .listing()
            .and_then(|l| l.entries().iter().position(|e| e.name == measured.name))
        else {
            return false;
        };
        let Some(details) = self.details.get(&measured.tab) else {
            return false;
        };
        let mut rows = details.to_vec();
        let Some(entry) = rows.get_mut(row) else {
            return false;
        };
        entry.size = measured.bytes;
        self.details.insert(measured.tab, rows.into());
        true
    }

    fn accept(&mut self, loaded: Loaded) -> bool {
        let Loaded {
            tab,
            location,
            request,
            result,
        } = loaded;
        let current = self
            .state
            .tab(tab)
            .is_some_and(|t| t.pending_request() == Some(request));
        if !current {
            return false;
        }
        let listing = match result {
            Ok(snapshot) => {
                self.errors.remove(&tab);
                self.details.insert(tab, snapshot.details.into());
                snapshot.listing
            }
            Err(error) => {
                self.errors.insert(tab, error);
                self.details.remove(&tab);
                Listing::new(Vec::new())
            }
        };
        self.event(Event::ListingLoaded {
            tab,
            location,
            request,
            listing,
        });
        self.restore_focus(tab);
        true
    }

    fn restore_focus(&mut self, tab: TabId) {
        let Some(name) = self.focus_after_load.remove(&tab) else {
            return;
        };
        let row = self
            .state
            .tab(tab)
            .and_then(|t| t.listing())
            .and_then(|l| l.entries().iter().position(|e| e.name == name));
        let active = self.state.active_tab().id() == tab;
        if let (Some(row), true) = (row, active) {
            let to = CursorMove::Row(row as u64);
            let _ = self.command(&Command::PaneCursorMove { to });
        }
    }
}

#[cfg(test)]
mod tests;
