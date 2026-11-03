use std::path::Path;

use tempfile::TempDir;

use super::*;
use crate::model::Entry;
use crate::service::{ClipSink, NoSealer, Sequence, SinkError, SystemClock};

struct NullSink;

impl ClipSink for NullSink {
    fn write_text(&self, _text: &str) -> Result<(), SinkError> {
        Ok(())
    }

    fn write_files(&self, _paths: &[PathBuf], _cut: bool) -> Result<(), SinkError> {
        Ok(())
    }

    fn write_private(&self, _text: &str) -> Result<Sequence, SinkError> {
        Ok(Sequence(0))
    }

    fn clear_if_sequence(&self, _sequence: Sequence) -> Result<bool, SinkError> {
        Ok(false)
    }
}

fn old_row(at_ms: i64) -> Entry {
    Entry {
        content: Content::Text("old".to_owned()),
        source_app: None,
        first_ms: at_ms,
        last_ms: at_ms,
        uses: 1,
    }
}

fn worker_with_old_row(path: &Path) -> (Worker, Arc<Shared>) {
    let limits = Limits::default();
    let mut store = ClipStore::open(path, limits).unwrap();
    store.record(&old_row(1)).unwrap();
    let shared = Arc::new(Shared::new());
    let ports = Ports {
        sink: Arc::new(NullSink),
        sealer: Arc::new(NoSealer),
        clock: Arc::new(SystemClock),
    };
    let mut worker = Worker::new(store, limits, ports, shared.clone());
    worker.refresh();
    assert_eq!(shared.snapshot().history.len(), 1);
    (worker, shared)
}

fn stored_rows(path: &Path) -> usize {
    ClipStore::open(path, Limits::default())
        .unwrap()
        .history(10)
        .unwrap()
        .len()
}

#[test]
fn a_wipe_that_loses_the_store_publishes_an_empty_snapshot_until_it_reopens() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("clipboard.db");
    let (mut worker, shared) = worker_with_old_row(&real);
    let before = shared.snapshot().revision;

    worker.path = dir.path().join("missing").join("clipboard.db");
    let (reply, answer) = std::sync::mpsc::channel();
    worker.handle(Msg::Wipe(reply));
    assert_eq!(answer.try_recv(), Ok(Err(FailKind::Store)));

    let snapshot = shared.snapshot();
    assert!(snapshot.history.is_empty());
    assert!(snapshot.revision > before);
    assert_eq!(shared.last_error(), Some(FailKind::Store));
    assert_eq!(stored_rows(&real), 1);

    worker.handle(Msg::Delete(0, std::sync::mpsc::channel().0));
    assert!(shared.snapshot().history.is_empty());
    assert!(shared.errors() >= 2);

    worker.path = real.clone();
    worker.handle(Msg::Delete(0, std::sync::mpsc::channel().0));
    assert_eq!(shared.snapshot().history.len(), 1);
}

#[cfg(unix)]
#[test]
fn a_wipe_refused_by_the_folder_keeps_the_rows_and_the_files() {
    use std::fs::{File, Permissions, remove_file, set_permissions};
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let real = dir.path().join("clipboard.db");
    let (mut worker, shared) = worker_with_old_row(&real);
    set_permissions(dir.path(), Permissions::from_mode(0o500)).unwrap();
    let probe = dir.path().join("probe");
    if File::create(&probe).is_ok() {
        let _ = remove_file(&probe);
        set_permissions(dir.path(), Permissions::from_mode(0o700)).unwrap();
        return;
    }

    let (reply, answer) = std::sync::mpsc::channel();
    worker.handle(Msg::Wipe(reply));
    let snapshot = shared.snapshot();
    set_permissions(dir.path(), Permissions::from_mode(0o700)).unwrap();

    assert_eq!(answer.try_recv(), Ok(Err(FailKind::Store)));
    assert_eq!(snapshot.history.len(), 1);
    assert_eq!(shared.last_error(), Some(FailKind::Store));
    assert_eq!(stored_rows(&real), 1);
    worker.handle(Msg::Delete(0, std::sync::mpsc::channel().0));
    assert_eq!(shared.snapshot().history.len(), 1);
}
