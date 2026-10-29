#![allow(clippy::unwrap_used, clippy::panic, reason = "test support code")]

use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use rusty_pdf::fake::{PAGES, PASSWORD, wait_for_idle};
use rusty_pdf::protocol::PdfError;
use rusty_pdf::{Document, Launcher};

const FAKE: &str = env!("CARGO_BIN_EXE_rusty-pdf-fake-helper");
const STALL: &str = "--pause-prepare=4000";
const PATIENCE: Duration = Duration::from_secs(10);

struct Watch(TcpListener);

impl Watch {
    fn new() -> Self {
        let listener = (41000..=41019)
            .find_map(|port| TcpListener::bind(("127.0.0.1", port)).ok())
            .unwrap();
        listener.set_nonblocking(true).unwrap();
        Self(listener)
    }

    fn arg(&self) -> OsString {
        format!("--connect={}", self.0.local_addr().unwrap()).into()
    }

    fn accept(&self, within: Duration) -> Option<TcpStream> {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            match self.0.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    return Some(stream);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("{error}"),
            }
        }
        None
    }
}

fn exited(stream: &mut TcpStream, within: Duration) -> bool {
    stream.set_read_timeout(Some(within)).unwrap();
    let mut buffer = [0; 8];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return false;
            }
            Err(_) => return true,
        }
    }
}

fn prepare_started(stream: &mut TcpStream) {
    stream.set_read_timeout(Some(PATIENCE)).unwrap();
    let mut signal = [0];
    stream.read_exact(&mut signal).unwrap();
    assert_eq!(&signal, b"P");
}

fn launcher(watch: &Watch, extra: Option<&str>, timeout: Duration) -> Launcher {
    let args = extra.map(OsString::from).into_iter().chain([watch.arg()]);
    Launcher::new(FAKE, args).with_timeout(timeout)
}

fn tracked(launcher: Launcher) -> (Launcher, Arc<Mutex<Vec<u32>>>) {
    let ids = Arc::new(Mutex::new(Vec::new()));
    let seen = ids.clone();
    let tracked = launcher.with_confinement(Arc::new(move |child| {
        seen.lock().unwrap().push(child.id());
        Ok(Box::new(()))
    }));
    (tracked, ids)
}

fn open(launcher: &Launcher, name: &str, password: Option<&str>) -> Result<Document, PdfError> {
    Document::open(launcher, Path::new(name), password)
}

fn marker() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "navo-pdf-first-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ))
}

#[test]
fn dropping_a_document_with_a_stalled_prepare_returns_at_once_and_the_helper_exits() {
    let watch = Watch::new();
    let launcher = launcher(&watch, Some("--gate-prepare=1"), Duration::from_secs(4));
    let document = open(&launcher, "a.pdf", None).unwrap();
    let mut helper = watch.accept(PATIENCE).unwrap();
    let started = Instant::now();
    drop(document);
    assert!(started.elapsed() < Duration::from_millis(250));
    prepare_started(&mut helper);
    assert!(exited(&mut helper, PATIENCE));
}

#[test]
fn shutdown_during_warm_up_returns_within_the_bound_and_the_helper_exits() {
    let watch = Watch::new();
    let launcher = launcher(&watch, Some(STALL), Duration::from_secs(30));
    let warming = {
        let launcher = launcher.clone();
        thread::spawn(move || launcher.prewarm())
    };
    let mut helper = watch.accept(PATIENCE).unwrap();
    prepare_started(&mut helper);
    let started = Instant::now();
    launcher.shutdown();
    assert!(started.elapsed() < Duration::from_millis(700));
    assert!(exited(&mut helper, PATIENCE));
    assert!(warming.join().unwrap().is_err());
}

#[test]
fn a_warm_helper_finishing_after_a_cold_one_was_returned_is_disposed() {
    let watch = Watch::new();
    let marker = marker();
    let pause = format!("--gate-first-prepare={}", marker.display());
    let (launcher, ids) = tracked(
        launcher(&watch, Some(&pause), Duration::from_secs(10))
            .with_warm_wait(Duration::from_millis(50)),
    );
    let warming = {
        let launcher = launcher.clone();
        thread::spawn(move || launcher.prewarm())
    };
    let mut warm = watch.accept(PATIENCE).unwrap();
    prepare_started(&mut warm);
    let mut document = open(&launcher, "a.pdf", None).unwrap();
    let mut cold = watch.accept(PATIENCE).unwrap();
    assert_eq!(document.page_count(), Ok(PAGES));
    drop(document);
    prepare_started(&mut cold);
    assert!(wait_for_idle(&launcher, PATIENCE));
    warm.write_all(b"R").unwrap();
    assert_eq!(warming.join().unwrap(), Ok(()));
    assert!(exited(&mut warm, PATIENCE));
    let mut reused = open(&launcher, "a.pdf", None).unwrap();
    assert_eq!(reused.page_count(), Ok(PAGES));
    assert_eq!(ids.lock().unwrap().len(), 2);
    drop(reused);
    launcher.shutdown();
    assert!(exited(&mut cold, PATIENCE));
    let _ = std::fs::remove_file(marker);
}

#[test]
fn two_simultaneous_opens_get_distinct_helpers() {
    let watch = Watch::new();
    let (launcher, ids) = tracked(launcher(&watch, None, Duration::from_secs(10)));
    launcher.prewarm().unwrap();
    let mut first = open(&launcher, "locked.pdf", Some(PASSWORD)).unwrap();
    let mut second = open(&launcher, "a.pdf", None).unwrap();
    assert_eq!(first.page_count(), Ok(PAGES));
    assert_eq!(second.page_count(), Ok(PAGES));
    let ids = ids.lock().unwrap();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    drop(ids);
    drop(first);
    drop(second);
    launcher.shutdown();
}

#[test]
fn a_warm_helper_that_failed_to_open_is_not_reused_and_the_next_open_succeeds() {
    let watch = Watch::new();
    let (launcher, ids) = tracked(launcher(&watch, None, Duration::from_secs(10)));
    launcher.prewarm().unwrap();
    let mut warm = watch.accept(PATIENCE).unwrap();
    assert_eq!(
        open(&launcher, "broken.pdf", None).err(),
        Some(PdfError::Corrupt)
    );
    assert!(exited(&mut warm, PATIENCE));
    let mut document = open(&launcher, "a.pdf", None).unwrap();
    assert_eq!(document.page_count(), Ok(PAGES));
    assert_eq!(ids.lock().unwrap().len(), 2);
    drop(document);
    launcher.shutdown();
}

#[test]
fn shutdown_disposes_a_helper_already_recycling_a_stalled_prepare() {
    let watch = Watch::new();
    let launcher = launcher(&watch, Some("--gate-prepare=1"), Duration::from_secs(15));
    let document = open(&launcher, "a.pdf", None).unwrap();
    let mut helper = watch.accept(PATIENCE).unwrap();
    drop(document);
    prepare_started(&mut helper);
    let started = Instant::now();
    launcher.shutdown();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(exited(&mut helper, Duration::from_secs(1)));
}

#[test]
fn shutdown_during_a_warm_probe_refuses_a_fallback_spawn() {
    let watch = Watch::new();
    let (launcher, ids) = tracked(launcher(
        &watch,
        Some("--gate-prepare=2"),
        Duration::from_secs(15),
    ));
    launcher.prewarm().unwrap();
    let mut helper = watch.accept(PATIENCE).unwrap();
    prepare_started(&mut helper);
    let opening = {
        let launcher = launcher.clone();
        thread::spawn(move || open(&launcher, "a.pdf", None).err())
    };
    prepare_started(&mut helper);
    launcher.shutdown();
    assert_eq!(opening.join().unwrap(), Some(PdfError::HelperDied));
    assert_eq!(ids.lock().unwrap().len(), 1);
    assert!(exited(&mut helper, PATIENCE));
}

#[test]
fn shutdown_during_spawn_is_not_missed() {
    let watch = Watch::new();
    let (entered, spawning) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let launcher =
        launcher(&watch, None, Duration::from_secs(15)).with_confinement(Arc::new(move |_| {
            entered.send(()).unwrap();
            released.lock().unwrap().recv_timeout(PATIENCE).unwrap();
            Ok(Box::new(()))
        }));
    let opening = {
        let launcher = launcher.clone();
        thread::spawn(move || open(&launcher, "a.pdf", None).err())
    };
    spawning.recv_timeout(PATIENCE).unwrap();
    let mut helper = watch.accept(PATIENCE).unwrap();
    launcher.shutdown();
    release.send(()).unwrap();
    assert_eq!(opening.join().unwrap(), Some(PdfError::HelperDied));
    assert!(exited(&mut helper, PATIENCE));
}

#[test]
fn shutdown_during_a_prewarm_spawn_is_not_missed() {
    let watch = Watch::new();
    let (entered, spawning) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let launcher =
        launcher(&watch, None, Duration::from_secs(15)).with_confinement(Arc::new(move |_| {
            entered.send(()).unwrap();
            released.lock().unwrap().recv_timeout(PATIENCE).unwrap();
            Ok(Box::new(()))
        }));
    let warming = {
        let launcher = launcher.clone();
        thread::spawn(move || launcher.prewarm())
    };
    spawning.recv_timeout(PATIENCE).unwrap();
    let mut helper = watch.accept(PATIENCE).unwrap();
    let started = Instant::now();
    launcher.shutdown();
    assert!(started.elapsed() < Duration::from_secs(1));
    release.send(()).unwrap();
    assert_eq!(warming.join().unwrap(), Err(PdfError::HelperDied));
    assert!(exited(&mut helper, PATIENCE));
}

#[test]
fn shutdown_leaves_another_launchers_recycling_helper_available() {
    let first_watch = Watch::new();
    let second_watch = Watch::new();
    let first = launcher(&first_watch, Some("--gate-prepare=1"), PATIENCE);
    let (second, ids) = tracked(launcher(&second_watch, Some("--gate-prepare=1"), PATIENCE));
    let first_document = open(&first, "a.pdf", None).unwrap();
    let second_document = open(&second, "a.pdf", None).unwrap();
    let mut first_helper = first_watch.accept(PATIENCE).unwrap();
    let mut second_helper = second_watch.accept(PATIENCE).unwrap();
    drop(first_document);
    drop(second_document);
    prepare_started(&mut first_helper);
    prepare_started(&mut second_helper);
    first.shutdown();
    assert!(exited(&mut first_helper, Duration::from_secs(1)));
    second_helper.write_all(b"R").unwrap();
    assert!(wait_for_idle(&second, PATIENCE));
    let mut document = open(&second, "a.pdf", None).unwrap();
    assert_eq!(document.page_count(), Ok(PAGES));
    assert_eq!(ids.lock().unwrap().len(), 1);
    drop(document);
    second.shutdown();
    assert!(exited(&mut second_helper, PATIENCE));
}
