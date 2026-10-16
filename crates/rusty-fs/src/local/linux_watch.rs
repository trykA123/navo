use std::ffi::OsStr;
use std::io;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use inotify::{Event, EventMask, Inotify, WatchDescriptor, WatchMask, Watches};
use rusty_core::{FsErrorKind, Location};

use super::{error, io_error, platform};
use crate::WatchChange;
use crate::watch::{RawKind, RawSender, Translate};

const BUFFER: usize = 16 * 1024;

const MASK: WatchMask = WatchMask::CREATE
    .union(WatchMask::DELETE)
    .union(WatchMask::MODIFY)
    .union(WatchMask::ATTRIB)
    .union(WatchMask::MOVED_FROM)
    .union(WatchMask::MOVED_TO)
    .union(WatchMask::DELETE_SELF)
    .union(WatchMask::MOVE_SELF)
    .union(WatchMask::ONLYDIR)
    .union(WatchMask::EXCL_UNLINK);

#[derive(Default)]
struct Shared {
    stopping: AtomicBool,
    registered: Mutex<Option<(Watches, WatchDescriptor)>>,
}

impl Shared {
    fn registered(&self) -> MutexGuard<'_, Option<(Watches, WatchDescriptor)>> {
        self.registered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

pub(super) struct FolderWatch {
    shared: Arc<Shared>,
    reader: Option<JoinHandle<()>>,
}

impl Drop for FolderWatch {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        if let Some((mut watches, descriptor)) = self.shared.registered().take() {
            let _ = watches.remove(descriptor);
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub(super) fn start(path: &Path, folder: &Location, raw: RawSender) -> io::Result<FolderWatch> {
    let shared = Arc::new(Shared::default());
    let (path, folder, opener) = (path.to_owned(), folder.clone(), Arc::clone(&shared));
    let reader = std::thread::Builder::new()
        .name("folder-watch-inotify".to_owned())
        .spawn(move || {
            if let Some(reader) = Reader::open(&path, folder, raw, opener) {
                reader.run();
            }
        })?;
    Ok(FolderWatch {
        shared,
        reader: Some(reader),
    })
}

struct Reader {
    inotify: Inotify,
    raw: RawSender,
    folder: Location,
    shared: Arc<Shared>,
}

impl Reader {
    fn open(path: &Path, folder: Location, raw: RawSender, shared: Arc<Shared>) -> Option<Self> {
        let added = Inotify::init().and_then(|inotify| {
            let mut watches = inotify.watches();
            let descriptor = watches.add(path, MASK)?;
            Ok((inotify, watches, descriptor))
        });
        let (inotify, watches, descriptor) = match added {
            Ok(added) => added,
            Err(failure) => {
                raw.failed(io_error(&failure, &folder));
                return None;
            }
        };
        let mut registered = shared.registered();
        if shared.stopping.load(Ordering::SeqCst) {
            return None;
        }
        *registered = Some((watches, descriptor));
        drop(registered);
        Some(Self {
            inotify,
            raw,
            folder,
            shared,
        })
    }

    fn run(mut self) {
        let mut buffer = vec![0u8; BUFFER];
        loop {
            let events = match self.inotify.read_events_blocking(&mut buffer) {
                Ok(events) => events,
                Err(failure) if failure.kind() == io::ErrorKind::Interrupted => continue,
                Err(failure) => return self.fail(io_error(&failure, &self.folder)),
            };
            let mut changes = Vec::new();
            let mut translate = Translate::default();
            let read = events
                .into_iter()
                .try_for_each(|event| self.read(&event, &mut translate, &mut changes));
            if read.is_break() {
                return;
            }
            translate.flush(&mut changes);
            if !changes.into_iter().all(|change| self.raw.change(change)) {
                return;
            }
        }
    }

    fn read(
        &self,
        event: &Event<&OsStr>,
        translate: &mut Translate,
        changes: &mut Vec<WatchChange>,
    ) -> ControlFlow<()> {
        let gone = EventMask::IGNORED | EventMask::DELETE_SELF | EventMask::MOVE_SELF;
        if event.mask.intersects(gone) {
            self.fail(error(FsErrorKind::NotFound, &self.folder));
            return ControlFlow::Break(());
        }
        if event.mask.intersects(EventMask::Q_OVERFLOW) {
            self.raw.overflow();
        } else if let (Some(kind), Some(name)) = (kind_of(event.mask, event.cookie), event.name) {
            translate.feed(kind, platform::entry_name(name), changes);
        }
        ControlFlow::Continue(())
    }

    fn fail(&self, failure: rusty_core::FsError) {
        if !self.shared.stopping.load(Ordering::SeqCst) {
            self.raw.failed(failure);
        }
    }
}

fn kind_of(mask: EventMask, cookie: u32) -> Option<RawKind> {
    if mask.intersects(EventMask::CREATE) {
        Some(RawKind::Created)
    } else if mask.intersects(EventMask::DELETE) {
        Some(RawKind::Removed)
    } else if mask.intersects(EventMask::MOVED_FROM) {
        Some(RawKind::MovedFrom(cookie))
    } else if mask.intersects(EventMask::MOVED_TO) {
        Some(RawKind::MovedTo(cookie))
    } else if mask.intersects(EventMask::MODIFY | EventMask::ATTRIB) {
        Some(RawKind::Modified)
    } else {
        None
    }
}
