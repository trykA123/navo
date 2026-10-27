use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rusty_shell::Drive;

const FRESH_FOR: Duration = Duration::from_secs(5);

type ListDrives = Arc<dyn Fn() -> Vec<Drive> + Send + Sync>;

#[derive(Default)]
struct Cached {
    drives: Arc<[Drive]>,
    listed: Option<Instant>,
    refreshing: bool,
}

#[derive(Clone)]
pub(super) struct DriveCache {
    list: ListDrives,
    cached: Arc<Mutex<Cached>>,
}

impl DriveCache {
    pub(super) fn new(list: ListDrives) -> Self {
        Self {
            list,
            cached: Arc::default(),
        }
    }

    pub(super) fn refresh_in_background(&self) {
        if std::mem::replace(&mut self.lock().refreshing, true) {
            return;
        }
        let cache = self.clone();
        let spawned = std::thread::Builder::new()
            .name("index-drive-list".into())
            .spawn(move || {
                cache.refresh();
                cache.lock().refreshing = false;
            });
        if spawned.is_err() {
            self.lock().refreshing = false;
        }
    }

    pub(super) fn drives(&self) -> Arc<[Drive]> {
        let listed = self.lock().listed;
        match listed {
            None => self.refresh(),
            Some(at) if at.elapsed() >= FRESH_FOR => self.refresh_in_background(),
            Some(_) => {}
        }
        self.lock().drives.clone()
    }

    fn refresh(&self) {
        let drives = (self.list)();
        let mut cached = self.lock();
        cached.drives = drives.into();
        cached.listed = Some(Instant::now());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Cached> {
        self.cached.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    use rusty_shell::DriveKind;

    use super::*;

    fn drive(root: &str) -> Drive {
        Drive {
            root: root.into(),
            letter: root.chars().next(),
            kind: DriveKind::Fixed,
            label: None,
            remote: None,
            connected: true,
            space: None,
        }
    }

    #[test]
    fn the_first_listing_leaves_a_running_background_refresh_marked_as_running() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (release, released) = mpsc::channel::<()>();
        let released = Mutex::new(released);
        let counted = calls.clone();
        let cache = DriveCache::new(Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            if std::thread::current().name() == Some("index-drive-list") {
                let _ = released
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5));
            }
            vec![drive(r"C:\")]
        }));
        cache.refresh_in_background();
        assert_eq!(cache.drives().len(), 1);
        assert!(cache.lock().refreshing);
        cache.lock().listed = Some(Instant::now() - FRESH_FOR);
        cache.drives();
        release.send(()).unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        while cache.lock().refreshing && Instant::now() < end {
            std::thread::yield_now();
        }
        assert!(!cache.lock().refreshing);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_stale_list_answers_at_once_while_one_refresh_runs_in_the_background() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (release, released) = mpsc::channel::<()>();
        let released = Mutex::new(released);
        let (refreshed, done) = mpsc::channel();
        let counted = calls.clone();
        let cache = DriveCache::new(Arc::new(move || {
            if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                return vec![drive(r"C:\")];
            }
            let _ = released.lock().unwrap().recv();
            let _ = refreshed.send(());
            vec![drive(r"C:\"), drive(r"E:\")]
        }));
        assert_eq!(cache.drives().len(), 1);
        cache.lock().listed = Some(Instant::now() - FRESH_FOR);
        let started = Instant::now();
        assert_eq!(cache.drives().len(), 1);
        assert_eq!(cache.drives().len(), 1);
        assert!(started.elapsed() < Duration::from_millis(50));
        release.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(5)).unwrap();
        while cache.lock().refreshing {
            std::thread::yield_now();
        }
        assert_eq!(cache.drives().len(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
