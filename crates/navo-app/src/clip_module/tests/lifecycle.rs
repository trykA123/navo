use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use super::super::Config;
use navo_shell::ClipboardError;
use navo_ui::clip_palette::{ClipMode, ClipboardHooks, How};
use tracing::field::{Field, Visit};
use tracing_subscriber::Registry;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

use super::super::listeners::Listeners;
use super::super::running::Running;
use super::super::{Backends, ClipModule, StartWatch};
use super::{PANIC, Rig, off, on};

#[derive(Default)]
struct Counts {
    starts: AtomicUsize,
    live: AtomicUsize,
}

struct Live(Arc<Counts>);

impl Drop for Live {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A watcher whose start waits for one token per call.
fn gated() -> (StartWatch, mpsc::Sender<()>, Arc<Counts>) {
    let (tx, rx) = mpsc::channel::<()>();
    let rx = Mutex::new(rx);
    let counts = Arc::new(Counts::default());
    let seen = counts.clone();
    let watch: StartWatch = Arc::new(move |_| {
        seen.starts.fetch_add(1, Ordering::SeqCst);
        rx.lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(20))
            .unwrap();
        seen.live.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Live(seen.clone())) as Box<dyn Send>)
    });
    (watch, tx, counts)
}

#[test]
fn apply_returns_while_the_store_is_still_opening() {
    let (watch, gate, counts) = gated();
    let rig = Rig::with_watch(Some(watch));
    rig.module.apply_config(on());
    assert!(rig.module.modes().is_empty(), "starting answers as off");
    assert_eq!(
        rig.module.search("", ClipMode::All, None).notice,
        Some(navo_l10n::tr("clipboard-busy"))
    );
    gate.send(()).unwrap();
    rig.module.settle();
    assert_eq!(rig.module.modes().len(), 4);
    assert_eq!(counts.live.load(Ordering::SeqCst), 1);
}

#[test]
fn a_start_overtaken_by_off_is_dropped_and_stopped() {
    let (watch, gate, counts) = gated();
    let rig = Rig::with_watch(Some(watch));
    rig.module.apply_config(on());
    rig.module.apply_config(off());
    gate.send(()).unwrap();
    rig.module.settle();
    assert!(rig.module.modes().is_empty());
    assert_eq!(counts.starts.load(Ordering::SeqCst), 1);
    assert_eq!(counts.live.load(Ordering::SeqCst), 0);
}

#[test]
fn quick_on_off_on_ends_on_exactly_once() {
    let (watch, gate, counts) = gated();
    let rig = Rig::with_watch(Some(watch));
    rig.module.apply_config(on());
    rig.module.apply_config(off());
    rig.module.apply_config(on());
    gate.send(()).unwrap();
    gate.send(()).unwrap();
    rig.module.settle();
    assert_eq!(rig.module.modes().len(), 4);
    assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
    assert_eq!(counts.live.load(Ordering::SeqCst), 1);
}

#[test]
fn revisions_only_rise_across_toggles() {
    let rig = Rig::new();
    let revision = || rig.module.search("", ClipMode::All, None).revision;
    rig.apply(on());
    rig.emit("first");
    rig.wait_for_title("first");
    let running = revision();
    rig.apply(off());
    let stopped = revision();
    rig.apply(on());
    let restarted = revision();
    rig.emit("second");
    rig.wait_for_title("second");
    let later = revision();
    assert!(running < stopped, "{running} then {stopped}");
    assert!(stopped < restarted, "{stopped} then {restarted}");
    assert!(restarted < later, "{restarted} then {later}");
}

#[test]
fn stop_ends_the_module_and_a_second_stop_is_harmless() {
    let rig = Rig::new();
    rig.apply(on());
    assert!(rig.emit("before quit"));
    rig.module.stop();
    assert!(!rig.emit("after quit"), "the watcher is gone");
    assert!(rig.module.modes().is_empty());
    assert_eq!(
        rig.module.clear_history(),
        Err(navo_l10n::tr("clipboard-off"))
    );
    rig.module.stop();
    assert!(rig.module.modes().is_empty());
}

#[test]
fn stop_waits_for_a_start_in_flight_and_stops_it() {
    let (watch, gate, counts) = gated();
    let rig = Rig::with_watch(Some(watch));
    rig.module.apply_config(on());
    gate.send(()).unwrap();
    rig.module.stop();
    assert_eq!(counts.starts.load(Ordering::SeqCst), 1);
    assert_eq!(counts.live.load(Ordering::SeqCst), 0);
    assert!(rig.module.modes().is_empty());
}

fn asides(rig: &Rig) -> usize {
    std::fs::read_dir(&rig.data)
        .unwrap()
        .filter(|entry| {
            let name = entry.as_ref().unwrap().file_name();
            name.to_string_lossy().contains(".corrupt-")
        })
        .count()
}

#[test]
fn delete_all_while_off_removes_rows_and_set_aside_copies() {
    let rig = Rig::new();
    rig.apply(on());
    rig.emit("old row");
    rig.wait_for_title("old row");
    rig.apply(off());
    std::fs::write(rig.data.join("clipboard.db.corrupt-1700000000000"), b"x").unwrap();
    assert_eq!(asides(&rig), 1);
    assert_eq!(rig.module.delete_all(), Ok(()));
    rig.module.settle();
    assert_eq!(asides(&rig), 0);
    assert!(rig.module.modes().is_empty(), "still off");
    rig.apply(on());
    assert!(rig.titles("").is_empty());
}

#[test]
fn delete_all_without_a_profile_is_a_no_op() {
    let rig = Rig::new();
    let module = ClipModule::new(None, Backends::shell());
    assert_eq!(module.delete_all(), Ok(()));
    module.settle();
    assert!(!rig.data.exists());
}

#[test]
fn a_watcher_that_fails_keeps_the_module_on_with_a_recording_notice() {
    let watch: StartWatch = Arc::new(|_| Err(ClipboardError::Busy));
    let rig = Rig::with_watch(Some(watch));
    rig.apply(on());
    assert_eq!(rig.module.modes().len(), 4);
    assert_eq!(
        notice(&rig),
        Some(navo_l10n::tr("clipboard-recording-failed"))
    );
}

fn notice(rig: &Rig) -> Option<String> {
    rig.module.search("", ClipMode::All, None).notice
}

#[test]
fn an_unsupported_watcher_gives_no_notice() {
    let watch: StartWatch = Arc::new(|_| Err(ClipboardError::Unsupported));
    let rig = Rig::with_watch(Some(watch));
    rig.apply(on());
    assert_eq!(notice(&rig), None);
}

#[test]
fn off_says_the_history_is_off_and_paused_says_so() {
    let rig = Rig::new();
    rig.apply(off());
    assert_eq!(notice(&rig), Some(navo_l10n::tr("clipboard-off")));
    rig.apply(Config {
        enabled: true,
        record: false,
    });
    assert_eq!(notice(&rig), Some(navo_l10n::tr("clipboard-paused")));
    rig.apply(on());
    assert_eq!(notice(&rig), None);
    rig.module.set_recording(false).unwrap();
    assert_eq!(notice(&rig), Some(navo_l10n::tr("clipboard-paused")));
}

#[test]
fn a_dead_worker_reads_as_a_failed_recorder() {
    let rig = Rig::new();
    rig.apply(on());
    rig.emit(PANIC);
    rig.wait_for_title(PANIC);
    let id = rig.module.search("", ClipMode::All, None).rows[0].id;
    assert!(rig.module.copy(id, How::Copy).is_err());
    let failed = navo_l10n::tr("clipboard-recording-failed");
    for _ in 0..1000 {
        if notice(&rig).as_ref() == Some(&failed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(notice(&rig), Some(failed.clone()));
    assert_eq!(rig.module.delete(id), Err(failed.clone()));
    assert_eq!(rig.module.delete_all(), Err(failed));
}

#[test]
fn delete_all_while_off_reports_a_file_it_cannot_delete() {
    let rig = Rig::new();
    rig.apply(on());
    rig.emit("old row");
    rig.wait_for_title("old row");
    rig.apply(off());
    // Bytes 60..64 of the database header hold its version.
    let mut bytes = std::fs::read(rig.db()).unwrap();
    bytes[60..64].copy_from_slice(&u32::MAX.to_be_bytes());
    std::fs::write(rig.db(), &bytes).unwrap();
    assert_eq!(
        rig.module.delete_all(),
        Err(navo_l10n::tr("clipboard-not-deleted"))
    );
    assert_eq!(std::fs::read(rig.db()).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn delete_all_while_on_reports_a_failed_wipe() {
    use std::fs::{File, Permissions, remove_file, set_permissions};
    use std::os::unix::fs::PermissionsExt;

    let rig = Rig::new();
    rig.apply(on());
    rig.emit("old row");
    rig.wait_for_title("old row");
    set_permissions(&rig.data, Permissions::from_mode(0o500)).unwrap();
    let probe = rig.data.join("probe");
    let writable = File::create(&probe).is_ok();
    let result = if writable {
        let _ = remove_file(&probe);
        None
    } else {
        Some(rig.module.delete_all())
    };
    set_permissions(&rig.data, Permissions::from_mode(0o700)).unwrap();
    if let Some(result) = result {
        assert_eq!(result, Err(navo_l10n::tr("clipboard-not-deleted")));
        assert_eq!(rig.titles(""), vec!["old row".to_owned()]);
    }
}

#[derive(Default, Clone)]
struct Lines(Arc<Mutex<Vec<String>>>);

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.push_str(&format!(" {}={value:?}", field.name()));
    }
}

impl<S: tracing::Subscriber> Layer<S> for Lines {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields(event.metadata().level().to_string());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
}

fn start_logs(error: ClipboardError) -> Vec<String> {
    let rig = Rig::new();
    let error = Mutex::new(Some(error));
    let watch: StartWatch = Arc::new(move |_| Err(error.lock().unwrap().take().unwrap()));
    let backends = Backends {
        watch,
        ports: Backends::shell().ports,
        secrets: false,
    };
    let lines = Lines::default();
    let subscriber = Registry::default().with(lines.clone());
    tracing::subscriber::with_default(subscriber, || {
        let running = Running::start(&rig.db(), true, &backends, &Arc::new(Listeners::default()));
        running.unwrap().stop();
    });
    lines.0.lock().unwrap().clone()
}

#[test]
fn a_failed_watcher_warns_with_its_kind_only() {
    let lines = start_logs(ClipboardError::Os("secret-ish os text".to_owned()));
    let warned: Vec<_> = lines
        .iter()
        .filter(|line| line.starts_with("WARN"))
        .collect();
    assert_eq!(warned.len(), 1, "{lines:?}");
    assert!(warned[0].contains("kind=\"os\""), "{lines:?}");
    assert!(!warned[0].contains("secret-ish"), "{lines:?}");
}

#[test]
fn an_unsupported_watcher_does_not_warn() {
    let lines = start_logs(ClipboardError::Unsupported);
    assert!(
        !lines.iter().any(|line| line.starts_with("WARN")),
        "{lines:?}"
    );
}
