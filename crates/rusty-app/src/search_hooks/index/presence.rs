use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_ui::command_center::IndexOptions;

const FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Selection {
    pub(super) drives: Vec<String>,
    pub(super) removable: bool,
}

impl Selection {
    pub(super) fn of(options: &IndexOptions) -> Self {
        Self {
            drives: options.drives.clone(),
            removable: options.removable,
        }
    }

    #[cfg(any(windows, test))]
    pub(super) fn search(&self) -> rusty_store::Search {
        rusty_store::Search {
            drives: self.drives.clone(),
            removable: self.removable,
            ..rusty_store::Search::default()
        }
    }
}

pub(super) trait ServiceManager: Send + Sync {
    fn installed_volume_roots(&self, selection: &Selection) -> io::Result<Option<Vec<String>>>;
}

pub(super) type Volumes = Option<Arc<[Location]>>;

#[derive(Default)]
struct Cached {
    known: Option<(Selection, Instant, Volumes)>,
    refreshing: bool,
}

#[derive(Clone)]
pub(super) struct ServicePresence {
    manager: Arc<dyn ServiceManager>,
    cached: Arc<Mutex<Cached>>,
}

impl ServicePresence {
    pub(super) fn new(manager: Arc<dyn ServiceManager>) -> Self {
        Self {
            manager,
            cached: Arc::default(),
        }
    }

    pub(super) fn system() -> Self {
        Self::new(Arc::new(SystemServices))
    }

    pub(super) fn volumes(&self, selection: &Selection) -> Option<Volumes> {
        let cached = self.lock();
        let known = cached
            .known
            .as_ref()
            .filter(|(seen, _, _)| seen == selection);
        let fresh = known.is_some_and(|(_, at, _)| at.elapsed() < FRESH_FOR);
        let volumes = known.map(|(_, _, volumes)| volumes.clone());
        drop(cached);
        if !fresh {
            self.refresh_in_background(selection);
        }
        volumes
    }

    pub(super) fn refresh_in_background(&self, selection: &Selection) {
        if std::mem::replace(&mut self.lock().refreshing, true) {
            return;
        }
        let presence = self.clone();
        let selection = selection.clone();
        let spawned = std::thread::Builder::new()
            .name("index-service-presence".into())
            .spawn(move || presence.refresh(selection));
        if spawned.is_err() {
            self.lock().refreshing = false;
        }
    }

    fn refresh(&self, selection: Selection) {
        let volumes = match self.manager.installed_volume_roots(&selection) {
            Ok(roots) => roots.map(|roots| {
                roots
                    .iter()
                    .filter_map(|root| Location::new(root).ok())
                    .collect()
            }),
            Err(error) => {
                tracing::warn!(%error, "cannot ask the service manager about the index service");
                None
            }
        };
        let mut cached = self.lock();
        cached.known = Some((selection, Instant::now(), volumes));
        cached.refreshing = false;
    }

    fn lock(&self) -> MutexGuard<'_, Cached> {
        self.cached.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct SystemServices;

impl ServiceManager for SystemServices {
    #[cfg(windows)]
    fn installed_volume_roots(&self, selection: &Selection) -> io::Result<Option<Vec<String>>> {
        crate::index_service::installed_volume_roots(&selection.search())
    }

    #[cfg(not(windows))]
    fn installed_volume_roots(&self, _: &Selection) -> io::Result<Option<Vec<String>>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    use super::*;

    struct Gated {
        calls: AtomicUsize,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl ServiceManager for Gated {
        fn installed_volume_roots(&self, _: &Selection) -> io::Result<Option<Vec<String>>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _ = self
                .release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5));
            Ok(Some(vec![r"C:\".into()]))
        }
    }

    fn gated() -> (Arc<Gated>, mpsc::Sender<()>) {
        let (release, released) = mpsc::channel();
        let manager = Arc::new(Gated {
            calls: AtomicUsize::new(0),
            release: Mutex::new(released),
        });
        (manager, release)
    }

    fn settle(presence: &ServicePresence) {
        let end = Instant::now() + Duration::from_secs(5);
        while presence.lock().refreshing && Instant::now() < end {
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_slow_service_manager_never_blocks_the_caller() {
        let (manager, release) = gated();
        let presence = ServicePresence::new(manager.clone());
        let selection = Selection::default();
        let started = Instant::now();
        assert_eq!(presence.volumes(&selection), None);
        assert_eq!(presence.volumes(&selection), None);
        assert!(started.elapsed() < Duration::from_secs(1));
        release.send(()).unwrap();
        settle(&presence);
        let known = presence.volumes(&selection).flatten();
        assert_eq!(known.map(|roots| roots.len()), Some(1));
        assert_eq!(manager.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_stale_answer_is_used_while_one_refresh_runs_every_thirty_seconds() {
        let (manager, release) = gated();
        let presence = ServicePresence::new(manager.clone());
        let selection = Selection::default();
        release.send(()).unwrap();
        presence.volumes(&selection);
        settle(&presence);
        presence.volumes(&selection);
        assert_eq!(manager.calls.load(Ordering::SeqCst), 1);
        if let Some((_, at, _)) = presence.lock().known.as_mut() {
            *at -= FRESH_FOR;
        }
        assert!(presence.volumes(&selection).flatten().is_some());
        assert!(presence.volumes(&selection).flatten().is_some());
        release.send(()).unwrap();
        settle(&presence);
        assert_eq!(manager.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn changed_drive_settings_are_unknown_until_asked_again() {
        let (manager, release) = gated();
        let presence = ServicePresence::new(manager.clone());
        release.send(()).unwrap();
        presence.volumes(&Selection::default());
        settle(&presence);
        let removable = Selection {
            removable: true,
            ..Selection::default()
        };
        assert_eq!(presence.volumes(&removable), None);
        release.send(()).unwrap();
        settle(&presence);
        assert!(presence.volumes(&removable).is_some());
    }
}
