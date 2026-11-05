use std::fs;
use std::sync::mpsc;
use std::time::Duration;

use navo_core::ProviderPath;

use super::*;
use crate::{Watch, WatchEvent};

fn watched(path: &Path) -> (Watch, mpsc::Receiver<WatchEvent>) {
    let folder = Location::local(ProviderPath::from_text(path.to_str().unwrap())).unwrap();
    let (tx, rx) = mpsc::channel();
    let watch = Watch::start(
        &folder,
        Box::new(move |event| {
            let _ = tx.send(event);
        }),
        |raw| {
            let shared = Arc::new(Shared::default());
            let reader = Reader::open(path, folder.clone(), raw, Arc::clone(&shared)).unwrap();
            // A 32-byte buffer fits exactly one short inotify name record.
            let reader = std::thread::spawn(move || reader.run_with_buffer(&mut [0; 32]));
            Ok(FolderWatch {
                shared,
                reader: Some(reader),
            })
        },
    )
    .unwrap();
    (watch, rx)
}

fn change(rx: &mpsc::Receiver<WatchEvent>, expected: WatchChange) {
    let event = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(
        matches!(event, WatchEvent::Changed(ref changes) if changes == &[expected]),
        "{event:?}"
    );
}

#[test]
fn a_rename_split_across_reads_stays_one_rename() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("before"), b"x").unwrap();
    let (_watch, rx) = watched(dir.path());
    fs::rename(dir.path().join("before"), dir.path().join("after")).unwrap();
    change(
        &rx,
        WatchChange::Renamed {
            from: ProviderPath::from_text("before"),
            to: ProviderPath::from_text("after"),
        },
    );
}

#[test]
fn a_move_out_without_a_destination_is_reported_while_idle() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("before"), b"x").unwrap();
    let (_watch, rx) = watched(dir.path());
    fs::rename(dir.path().join("before"), outside.path().join("after")).unwrap();
    change(&rx, WatchChange::Removed(ProviderPath::from_text("before")));
}

#[test]
fn dropping_a_watch_with_a_pending_move_stops_its_reader() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("before"), b"x").unwrap();
    let (watch, _rx) = watched(dir.path());
    fs::rename(dir.path().join("before"), outside.path().join("after")).unwrap();
    drop(watch);
}
