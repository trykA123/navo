use std::sync::{Arc, mpsc};
use std::time::Duration;

use navo_shell::ClipboardError;
use navo_ui::clip_palette::{ClipId, ClipMode, ClipboardHooks, How};
use navo_ui::settings::{Change, Field, Value, Values};

use super::super::{Config, StartWatch};
use super::{Rig, SLOW, off, on, wait_until_len};

#[test]
fn enabled_opens_the_file_and_offers_the_five_modes() {
    let rig = Rig::new();
    rig.apply(on());
    assert!(rig.db().exists());
    assert_eq!(
        rig.module.modes(),
        vec![
            ClipMode::All,
            ClipMode::Recent,
            ClipMode::Saved,
            ClipMode::Secrets,
            ClipMode::Notes
        ]
    );
}

#[test]
fn disabled_opens_no_file_and_refuses() {
    let rig = Rig::new();
    rig.apply(off());
    assert!(!rig.db().exists());
    assert!(rig.module.modes().is_empty());
    assert_eq!(
        rig.module.copy(ClipId(1), How::Copy),
        Err(navo_l10n::tr("clipboard-off"))
    );
    assert!(!rig.emit("never watched"));
}

#[test]
fn settings_values_start_and_stop_the_module() {
    let rig = Rig::new();
    let values = Values::default();
    rig.module.apply(&values);
    rig.module.settle();
    assert!(rig.db().exists());
    let off = values.with(&Change {
        field: Field::ClipboardOn,
        value: Value::Flag(false),
    });
    rig.module.apply(&off);
    rig.module.settle();
    assert!(rig.module.modes().is_empty());
}

#[test]
fn a_captured_text_reaches_search() {
    let rig = Rig::new();
    rig.apply(on());
    assert!(rig.emit("alpha note\nsecond line"));
    rig.wait_for_title("alpha note");
    assert!(rig.titles("beta").is_empty());
    let results = rig.module.search("note", ClipMode::Recent, None);
    assert_eq!(results.rows.len(), 1);
    assert_eq!(results.rows[0].ranges, vec![6..10]);
    assert_eq!(results.kept, 1);
    assert_eq!(results.rows[0].source_app.as_deref(), Some("editor"));
}

#[test]
fn toggling_off_and_on_keeps_the_rows() {
    let rig = Rig::new();
    rig.apply(on());
    rig.emit("kept across a toggle");
    rig.wait_for_title("kept across a toggle");
    rig.apply(off());
    assert!(rig.titles("").is_empty());
    assert!(rig.db().exists());
    rig.apply(on());
    assert_eq!(rig.titles(""), vec!["kept across a toggle".to_owned()]);
}

#[test]
fn a_damaged_file_is_set_aside_and_the_notice_has_no_path() {
    let rig = Rig::new();
    std::fs::create_dir_all(&rig.data).unwrap();
    std::fs::write(rig.db(), [0x5a_u8; 4096]).unwrap();
    rig.apply(on());
    let results = rig.module.search("", ClipMode::All, None);
    let notice = results.notice.unwrap();
    assert_eq!(notice, navo_l10n::tr("clipboard-moved-aside"));
    assert!(!notice.contains(rig.data.to_str().unwrap()));
    let asides = std::fs::read_dir(&rig.data)
        .unwrap()
        .filter(|entry| {
            let name = entry.as_ref().unwrap().file_name();
            name.to_string_lossy().contains(".corrupt-")
        })
        .count();
    assert_eq!(asides, 1);
    assert_eq!(rig.module.modes().len(), 5);
}

#[test]
fn a_store_that_cannot_open_stays_off_with_a_notice() {
    let rig = Rig::new();
    std::fs::create_dir_all(rig.db()).unwrap();
    rig.apply(on());
    assert!(rig.module.modes().is_empty());
    let results = rig.module.search("", ClipMode::All, None);
    assert_eq!(results.notice, Some(navo_l10n::tr("clipboard-open-failed")));
    assert!(rig.db().is_dir());
}

#[test]
fn record_history_off_drops_captures() {
    let rig = Rig::new();
    rig.apply(Config {
        enabled: true,
        record: false,
    });
    rig.emit("dropped while paused");
    rig.apply(on());
    rig.emit("kept after resume");
    rig.wait_for_title("kept after resume");
    assert_eq!(rig.titles(""), vec!["kept after resume".to_owned()]);
}

#[test]
fn the_palette_can_pause_and_resume_recording() {
    let rig = Rig::new();
    rig.apply(on());
    rig.module.set_recording(false).unwrap();
    rig.emit("dropped while paused");
    rig.module.set_recording(true).unwrap();
    rig.emit("kept after resume");
    rig.wait_for_title("kept after resume");
    assert_eq!(rig.titles("").len(), 1);
}

#[test]
fn a_missing_watcher_leaves_the_module_on() {
    let watch: StartWatch = Arc::new(|_| Err(ClipboardError::Unsupported));
    let rig = Rig::with_watch(Some(watch));
    rig.apply(on());
    assert_eq!(rig.module.modes().len(), 5);
}

#[test]
fn stopping_drops_the_watcher() {
    let rig = Rig::new();
    rig.apply(on());
    assert!(rig.emit("a"));
    rig.apply(off());
    assert!(!rig.emit("b"));
}

#[test]
fn subscribers_hear_changes_and_survive_a_restart() {
    let rig = Rig::new();
    let (tx, rx) = mpsc::channel();
    rig.module.subscribe(Box::new(move || {
        let _ = tx.send(());
    }));
    rig.apply(on());
    assert!(rx.try_recv().is_ok(), "start notifies");
    while rx.try_recv().is_ok() {}
    rig.apply(off());
    assert!(rx.try_recv().is_ok(), "stop notifies");
    rig.apply(on());
    while rx.try_recv().is_ok() {}
    rig.emit("after the restart");
    assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
}

#[test]
fn copy_returns_once_the_sink_has_written_the_row() {
    let rig = Rig::new();
    rig.apply(on());
    let text = format!("{SLOW} copy me");
    rig.emit(&text);
    rig.wait_for_title(&text);
    let id = rig.module.search("", ClipMode::All, None).rows[0].id;
    rig.module.copy(id, How::Paste).unwrap();
    assert_eq!(*rig.sink.texts.lock().unwrap(), vec![text]);
}

#[test]
fn copy_of_a_row_that_is_gone_reports_it() {
    let rig = Rig::new();
    rig.apply(on());
    rig.emit("short lived");
    rig.wait_for_title("short lived");
    let id = rig.module.search("", ClipMode::All, None).rows[0].id;
    rig.module.clear_history().unwrap();
    assert_eq!(
        rig.module.copy(id, How::Copy),
        Err(navo_l10n::tr("clipboard-not-found"))
    );
    assert!(rig.sink.texts.lock().unwrap().is_empty());
}

#[test]
fn delete_clear_and_delete_all_remove_rows() {
    let rig = Rig::new();
    rig.apply(on());
    for text in ["one", "two", "three"] {
        rig.emit(text);
        rig.wait_for_title(text);
    }
    let id = rig.module.search("two", ClipMode::All, None).rows[0].id;
    rig.module.delete(id).unwrap();
    wait_until_len(&rig, 2);
    rig.module.clear_history().unwrap();
    wait_until_len(&rig, 0);
    rig.emit("again");
    rig.wait_for_title("again");
    rig.module.delete_all().unwrap();
    wait_until_len(&rig, 0);
    assert!(rig.db().exists());
}

#[test]
fn a_history_row_that_cannot_be_deleted_reads_not_deleted() {
    let rig = Rig::new();
    rig.apply(on());
    rig.emit("kept");
    rig.wait_for_title("kept");
    let id = rig.module.search("kept", ClipMode::All, None).rows[0].id;
    rig.refuse_deletes("clips");
    assert_eq!(
        rig.module.delete(id),
        Err(navo_l10n::tr("clipboard-not-deleted"))
    );
    assert_eq!(rig.titles(""), ["kept"]);
}
