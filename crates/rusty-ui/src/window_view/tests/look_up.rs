use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rusty_core::{FsError, FsErrorKind, Location, ProviderId};
use rusty_fs::{CancelToken, Capabilities, FakeFs, FsEntry, ListSink, Provider};

use super::super::hand_off::{check, look_ups};

struct Held {
    inner: Arc<FakeFs>,
    is_open: AtomicBool,
    entered: AtomicUsize,
    busy: AtomicUsize,
    most: AtomicUsize,
}

impl Held {
    fn count(counter: &AtomicUsize) -> usize {
        counter.load(Ordering::SeqCst)
    }

    fn wait_until(&self, what: &str, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Provider for Held {
    fn id(&self) -> ProviderId {
        self.inner.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.inner.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.entered.fetch_add(1, Ordering::SeqCst);
        let busy = self.busy.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(busy, Ordering::SeqCst);
        while !self.is_open.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.busy.fetch_sub(1, Ordering::SeqCst);
        self.inner.metadata(location)
    }
}

#[test]
fn ten_look_ups_on_a_hung_path_keep_one_thread_busy_until_it_answers() {
    let fs = Arc::new(FakeFs::new());
    fs.add_file("share/report.txt", 3);
    let held = Arc::new(Held {
        inner: fs.clone(),
        is_open: AtomicBool::new(false),
        entered: AtomicUsize::new(0),
        busy: AtomicUsize::new(0),
        most: AtomicUsize::new(0),
    });
    let flights = look_ups(Duration::from_millis(50));
    let file = fs.location("share/report.txt");
    let callers: Vec<_> = (0..10)
        .map(|_| {
            let (flights, held, file) = (flights.clone(), held.clone(), file.clone());
            std::thread::spawn(move || check(&flights, held, &file).is_none())
        })
        .collect();
    for caller in callers {
        assert!(caller.join().expect("caller"));
    }
    assert_eq!(Held::count(&held.entered), 1);
    assert!(check(&flights, held.clone(), &fs.location("share")).is_none());
    held.is_open.store(true, Ordering::SeqCst);
    held.wait_until("the hung look-up to free its flight", |_| {
        !flights.is_unreachable(&file)
    });
    let found = check(&flights, held.clone(), &file).expect("an answer");
    assert_eq!(found.expect("the file").size, 3);
    assert_eq!(Held::count(&held.most), 1);
    assert_eq!(Held::count(&held.entered), 2);
}

struct Panics {
    inner: Arc<FakeFs>,
    calls: AtomicUsize,
}

impl Provider for Panics {
    fn id(&self) -> ProviderId {
        self.inner.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.inner.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        assert!(
            self.calls.fetch_add(1, Ordering::SeqCst) > 0,
            "first look-up fails"
        );
        self.inner.metadata(location)
    }
}

#[test]
fn a_look_up_that_panics_does_not_hold_the_flight() {
    let fs = Arc::new(FakeFs::new());
    fs.add_file("share/report.txt", 3);
    let provider = Arc::new(Panics {
        inner: fs.clone(),
        calls: AtomicUsize::new(0),
    });
    let flights = look_ups(Duration::from_secs(10));
    let file = fs.location("share/report.txt");
    assert!(check(&flights, provider.clone(), &file).is_none());
    let found = check(&flights, provider, &fs.location("share/report.txt"));
    assert!(found.is_some_and(|found| found.is_ok()));
}

#[derive(Default)]
struct Shares {
    is_open: AtomicBool,
    entered: AtomicUsize,
    busy: AtomicUsize,
    most: AtomicUsize,
}

impl Provider for Shares {
    fn id(&self) -> ProviderId {
        ProviderId::local()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn list(
        &self,
        _folder: &Location,
        _sink: &mut dyn ListSink,
        _cancel: &CancelToken,
    ) -> Result<(), FsError> {
        Ok(())
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        if location.as_str().starts_with(r"\\") {
            self.entered.fetch_add(1, Ordering::SeqCst);
            let busy = self.busy.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(busy, Ordering::SeqCst);
            while !self.is_open.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(2));
            }
            self.busy.fetch_sub(1, Ordering::SeqCst);
        }
        Err(FsError {
            location: Some(location.clone()),
            ..FsError::new(FsErrorKind::NotFound)
        })
    }
}

fn place(text: &str) -> Location {
    Location::new(text).expect("test locations are valid")
}

#[test]
fn a_hung_share_does_not_delay_a_local_drive() {
    let shares = Arc::new(Shares::default());
    let flights = look_ups(Duration::from_secs(2));
    let hung = (flights.clone(), shares.clone());
    let waiter = std::thread::spawn(move || check(&hung.0, hung.1, &place(r"\\nas\share\x")));
    let count = |counter: &AtomicUsize| counter.load(Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(10);
    while count(&shares.entered) == 0 {
        assert!(Instant::now() < deadline, "the share look-up never started");
        std::thread::sleep(Duration::from_millis(2));
    }
    let started = Instant::now();
    let local = check(&flights, shares.clone(), &place(r"C:\Work"));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(
        local.is_some_and(|found| found.is_err_and(|error| error.kind == FsErrorKind::NotFound))
    );
    shares.is_open.store(true, Ordering::SeqCst);
    waiter.join().expect("waiter");
}

#[test]
fn hung_roots_use_at_most_four_threads_and_the_fifth_gets_none_after_the_wait() {
    let shares = Arc::new(Shares::default());
    let wait = Duration::from_millis(200);
    let flights = look_ups(wait);
    for server in ["a", "b", "c", "d"] {
        let share = place(&format!(r"\\{server}\share\x"));
        assert!(check(&flights, shares.clone(), &share).is_none());
    }
    let started = Instant::now();
    assert!(check(&flights, shares.clone(), &place(r"\\e\share\x")).is_none());
    assert!(started.elapsed() >= wait);
    let started = Instant::now();
    assert!(check(&flights, shares.clone(), &place(r"\\?\UNC\a\share\y")).is_none());
    assert!(started.elapsed() < wait);
    assert_eq!(shares.entered.load(Ordering::SeqCst), 4);
    assert_eq!(shares.most.load(Ordering::SeqCst), 4);
    shares.is_open.store(true, Ordering::SeqCst);
}
