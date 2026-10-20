use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use rusty_store::Settings;
use rusty_ui::settings::{Change, DefaultShift, Field, Value};
use rusty_ui::window_view::SettingsNews;

use super::{follow_defaults, spawn, start, start_sharing, watched};

const WAIT: Duration = Duration::from_secs(10);
const NOTE: &str = "# keep this note\n";

struct Paths {
    _dir: tempfile::TempDir,
    file: PathBuf,
    ledger: PathBuf,
}

fn installed() -> Paths {
    let dir = tempfile::tempdir().unwrap();
    let at = Paths {
        file: dir.path().join("ferrite.toml"),
        ledger: dir.path().join("ferrite.db"),
        _dir: dir,
    };
    fs::write(&at.file, NOTE).unwrap();
    let _ = start(Some(&at.file), Some(&at.ledger));
    at
}

fn newer_defaults() -> Settings {
    let mut defaults = Settings::default();
    defaults.appearance.font_size = 18;
    defaults
}

fn font_size_shift() -> DefaultShift {
    DefaultShift {
        key: "appearance.font-size".to_owned(),
        old: "13".to_owned(),
        new: "18".to_owned(),
    }
}

fn next_defaults(news: &Receiver<SettingsNews>) -> Vec<DefaultShift> {
    loop {
        if let SettingsNews::Defaults(shifts) = news.recv_timeout(WAIT).unwrap() {
            return shifts;
        }
    }
}

#[test]
fn the_start_up_default_changes_reach_the_notice() {
    let at = installed();
    let (file, ledger) = (at.file.clone(), at.ledger.clone());
    let starting = spawn(Some(at.file.clone()), WAIT, move |backup| {
        let mut started = start_sharing(Some(&file), None, backup);
        started.default_changes = follow_defaults(
            &mut started.loaded.as_mut().unwrap().store,
            &ledger,
            &newer_defaults(),
        );
        started
    });
    let loaded = starting.finish().into_loaded().unwrap();
    let (send, news) = mpsc::channel();
    let (_hooks, _watch) = watched(Some(loaded), move |event| {
        let _ = send.send(event);
    });
    assert_eq!(next_defaults(&news), [font_size_shift()]);
}

#[test]
fn a_late_completion_still_reaches_the_notice() {
    let at = installed();
    let read = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (read_worker, gate) = (read.clone(), release.clone());
    let (file, ledger) = (at.file.clone(), at.ledger.clone());
    let starting = spawn(Some(at.file.clone()), Duration::ZERO, move |backup| {
        let mut started = start_sharing(Some(&file), None, backup);
        read_worker.wait();
        gate.wait();
        started.default_changes = follow_defaults(
            &mut started.loaded.as_mut().unwrap().store,
            &ledger,
            &newer_defaults(),
        );
        started
    });
    read.wait();
    let started = starting.finish();
    assert_eq!(started.default_changes, []);
    let (send, news) = mpsc::channel();
    let (_hooks, _watch) = watched(started.into_loaded(), move |event| {
        let _ = send.send(event);
    });
    release.wait();
    assert_eq!(next_defaults(&news), [font_size_shift()]);
}

#[test]
fn keep_old_value_writes_the_old_value_as_the_users_and_the_file_shows_it() {
    let at = installed();
    let (file, ledger) = (at.file.clone(), at.ledger.clone());
    let starting = spawn(Some(at.file.clone()), WAIT, move |backup| {
        let mut started = start_sharing(Some(&file), None, backup);
        started.default_changes = follow_defaults(
            &mut started.loaded.as_mut().unwrap().store,
            &ledger,
            &newer_defaults(),
        );
        started
    });
    let loaded = starting.finish().into_loaded().unwrap();
    let (send, news) = mpsc::channel();
    let (hooks, _watch) = watched(Some(loaded), move |event| {
        let _ = send.send(event);
    });
    let shifts = next_defaults(&news);
    let moved = rusty_ui::settings::Moved::of(&shifts[0]).unwrap();
    let kept = moved.keep_change().unwrap();
    assert_eq!(
        kept,
        Change {
            field: Field::FontSize,
            value: Value::Number(13)
        }
    );
    assert_eq!(
        fs::read_to_string(&at.file)
            .unwrap()
            .matches("font-size = 18")
            .count(),
        1
    );
    let report = (hooks.save)(&[kept]);
    assert!(report.error.is_none(), "{:?}", report.error);
    let text = fs::read_to_string(&at.file).unwrap();
    assert!(text.starts_with(NOTE), "{text}");
    assert!(text.contains("font-size = 13"), "{text}");
    assert!(!text.contains("font-size = 18"), "{text}");
    assert_eq!(report.values.get(Field::FontSize), &Value::Number(13));
}

#[test]
fn a_kept_value_is_not_followed_again_and_raises_no_notice() {
    let at = installed();
    let (file, ledger) = (at.file.clone(), at.ledger.clone());
    let defaults = newer_defaults();
    let mut started = start_sharing(Some(&file), None, rusty_store::SessionBackup::default());
    let changes = follow_defaults(
        &mut started.loaded.as_mut().unwrap().store,
        &ledger,
        &defaults,
    );
    assert_eq!(changes.len(), 1);
    let loaded = started.loaded.unwrap();
    let (hooks, _watch) = watched(Some(loaded), |_| {});
    let kept = Change {
        field: Field::FontSize,
        value: Value::Number(13),
    };
    assert!((hooks.save)(&[kept]).error.is_none());
    let mut again = start_sharing(Some(&file), None, rusty_store::SessionBackup::default());
    let changes = follow_defaults(
        &mut again.loaded.as_mut().unwrap().store,
        &ledger,
        &defaults,
    );
    assert_eq!(changes, []);
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .contains("font-size = 13")
    );
}
