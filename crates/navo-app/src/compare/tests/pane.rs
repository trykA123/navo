use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use navo_compare::model::{ContentKey, HashCache};
use navo_compare::session::CheckMode;
use navo_ui::browser::{CompareHooks, CompareSink, CompareUpdate, RowStatus};
use navo_ui::settings::{Change, Field, Value, Values};

use super::{location, write};
use crate::compare::convert::settings_of;
use crate::compare::pane::PaneCompare;

const WAIT: Duration = Duration::from_secs(20);

struct NoCache;

impl HashCache for NoCache {
    fn get(&self, _key: &ContentKey) -> Option<[u8; 32]> {
        None
    }

    fn put(&self, _key: &ContentKey, _hash: [u8; 32]) {}
}

fn sink() -> (CompareSink, Receiver<CompareUpdate>) {
    let (sender, receiver) = mpsc::channel();
    let sink: CompareSink = Arc::new(move |update| {
        let _ = sender.send(update);
    });
    (sink, receiver)
}

/// The last status of every row path, until the first generation finishes.
fn finished(receiver: &Receiver<CompareUpdate>) -> Vec<(String, RowStatus)> {
    let mut rows: Vec<(String, RowStatus)> = Vec::new();
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            CompareUpdate::Rows { rows: batch, .. } => {
                for row in batch {
                    rows.retain(|(path, _)| *path != row.path);
                    rows.push((row.path, row.status));
                }
            }
            CompareUpdate::Totals { .. }
            | CompareUpdate::Stopped { .. }
            | CompareUpdate::Continued { .. } => {}
            CompareUpdate::Finished { .. } => break,
            CompareUpdate::Failed(text) => panic!("compare failed: {text}"),
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

#[test]
fn a_pane_compare_sends_plain_rows_until_it_finishes() {
    let (left, right) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(left.path(), "same.txt", b"one");
    write(right.path(), "same.txt", b"one");
    write(left.path(), "sub/diff.txt", b"aaa");
    write(right.path(), "sub/diff.txt", b"bbb");
    write(left.path(), "left.txt", b"l");
    let hooks = PaneCompare::with_cache(Arc::new(NoCache));
    let (sink, receiver) = sink();
    let sides = (&location(left.path()), &location(right.path()));
    let _control = hooks.start(sides, &Values::default(), sink);
    let rows = finished(&receiver);
    let expected = [
        ("left.txt", RowStatus::OnlyLeft),
        ("same.txt", RowStatus::Same),
        ("sub", RowStatus::Same),
        ("sub/diff.txt", RowStatus::Different),
    ];
    let expected: Vec<(String, RowStatus)> = expected
        .iter()
        .map(|(path, status)| ((*path).to_owned(), *status))
        .collect();
    assert_eq!(rows, expected);
}

#[test]
fn a_missing_folder_fails_with_the_could_not_start_text() {
    let right = tempfile::tempdir().unwrap();
    let missing = right.path().join("gone");
    let hooks = PaneCompare::with_cache(Arc::new(NoCache));
    let (sink, receiver) = sink();
    let sides = (&location(&missing), &location(right.path()));
    let _control = hooks.start(sides, &Values::default(), sink);
    let update = receiver.recv_timeout(WAIT).unwrap();
    let text = navo_l10n::tr("compare-could-not-start");
    assert_eq!(update, CompareUpdate::Failed(text));
}

#[test]
fn dropping_the_control_returns_at_once_and_ends_the_forwarding() {
    let (left, right) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for at in 0..200 {
        write(left.path(), &format!("f{at}.txt"), b"x");
        write(right.path(), &format!("f{at}.txt"), b"y");
    }
    let hooks = PaneCompare::with_cache(Arc::new(NoCache));
    let (sink, _receiver) = sink();
    let sides = (&location(left.path()), &location(right.path()));
    let control = hooks.start(sides, &Values::default(), Arc::clone(&sink));
    let dropped = Instant::now();
    drop(control);
    assert!(dropped.elapsed() < Duration::from_millis(200));
    let deadline = Instant::now() + WAIT;
    while Arc::strong_count(&sink) > 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(Arc::strong_count(&sink), 1);
}

#[test]
fn the_compare_values_map_through_the_stored_settings_rules() {
    let changes = [
        (Field::CompareContentCheck, Value::Pick(1)),
        (Field::CompareBudgetMib, Value::Number(100)),
        (
            Field::CompareIgnoreList,
            Value::Text("*.{o,obj}, target".into()),
        ),
        (Field::CompareRespectGitignore, Value::Flag(false)),
    ];
    let values = changes
        .into_iter()
        .fold(Values::default(), |values, (field, value)| {
            values.with(&Change { field, value })
        });
    let settings = settings_of(&values);
    assert_eq!(settings.content_check, CheckMode::SizeTime);
    assert_eq!(settings.memory_budget_mib, 256);
    assert_eq!(settings.ignore_list, ["*.{o,obj}", "target"]);
    assert!(!settings.respect_gitignore);
}

#[test]
fn default_values_give_the_default_compare_settings() {
    let settings = settings_of(&Values::default());
    assert_eq!(
        settings,
        crate::compare::settings::CompareSettings::default()
    );
}

#[test]
fn a_folder_row_carries_the_flag_of_a_difference_below_it() {
    let (left, right) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(left.path(), "a/b/x.txt", b"one");
    write(right.path(), "a/b/x.txt", b"two");
    write(left.path(), "c/y.txt", b"same");
    write(right.path(), "c/y.txt", b"same");
    let hooks = PaneCompare::with_cache(Arc::new(NoCache));
    let (sink, receiver) = sink();
    let sides = (&location(left.path()), &location(right.path()));
    let _control = hooks.start(sides, &Values::default(), sink);
    let mut flagged = std::collections::BTreeMap::new();
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            CompareUpdate::Rows { rows, .. } => {
                for row in rows {
                    flagged.insert(row.path, row.contains_differences);
                }
            }
            CompareUpdate::Finished { .. } => break,
            _ => {}
        }
    }
    let flagged: Vec<(&str, bool)> = flagged.iter().map(|(p, f)| (p.as_str(), *f)).collect();
    assert_eq!(
        flagged,
        [
            ("a", true),
            ("a/b", true),
            ("a/b/x.txt", false),
            ("c", false),
            ("c/y.txt", false)
        ]
    );
}

#[test]
fn only_local_panes_can_be_compared() {
    let hooks = PaneCompare::with_cache(Arc::new(NoCache));
    let dir = tempfile::tempdir().unwrap();
    assert!(hooks.supports(&location(dir.path())));
    let archive = navo_core::Location::new(r"archive://C:\a.zip").unwrap();
    let ftp = navo_core::Location::new("ftp://host/pub").unwrap();
    assert!(!hooks.supports(&archive) && !hooks.supports(&ftp));
}
