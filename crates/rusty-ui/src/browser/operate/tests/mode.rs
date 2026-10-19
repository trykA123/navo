use rusty_fs::CopyMode;

use std::sync::atomic::Ordering;
use std::thread::ThreadId;

use super::concurrent::{counting_browser, many_files};
use super::pause::gated;
use super::*;
use crate::browser::operate::work::worker_count;

fn network(_: &rusty_core::Location) -> DriveClass {
    DriveClass::Network
}

#[test]
fn the_job_runner_gets_one_worker_in_standard_mode() {
    let fs = fake();
    let place = fs.location("dest");
    let count = |mode| worker_count(mode, Some(&place), Some(&place), network);
    assert_eq!(count(CopyMode::Standard), 1);
    assert_eq!(count(CopyMode::Auto), 8);
}

#[test]
fn a_new_browser_copies_in_auto_mode_until_the_setting_says_otherwise() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    assert_eq!(browser.copy_mode(), CopyMode::Auto);
    browser.set_copy_mode(CopyMode::Standard);
    assert_eq!(browser.copy_mode(), CopyMode::Standard);
}

fn copy_in(mode: CopyMode, want: usize) -> (usize, usize) {
    let fs = many_files();
    let (mut browser, tally) = counting_browser(&fs, want);
    browser.set_copy_mode(mode);
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Done, "{mode:?}");
    let copied = (0..5)
        .filter(|index| exists(&fs, &format!("dest/docs/f{index}.txt")))
        .count();
    (
        copied + usize::from(exists(&fs, "dest/docs/report.txt")),
        tally.peak(),
    )
}

#[test]
fn auto_mode_copies_with_four_workers_on_an_ssd() {
    assert_eq!(copy_in(CopyMode::Auto, 4), (6, 4));
}

#[test]
fn standard_mode_copies_one_file_at_a_time() {
    assert_eq!(copy_in(CopyMode::Standard, 1), (6, 1));
}

#[test]
fn a_mode_change_after_confirm_leaves_the_running_job_alone() {
    let fs = many_files();
    let (mut browser, tally) = counting_browser(&fs, 4);
    let gate = gated(&mut browser);
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    browser.set_copy_mode(CopyMode::Standard);
    gate.store(true, Ordering::Release);
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    assert_eq!(tally.peak(), 4);
}

static PROBED_ON: Mutex<Vec<ThreadId>> = Mutex::new(Vec::new());

fn probing(_: &rusty_core::Location) -> DriveClass {
    PROBED_ON.lock().unwrap().push(std::thread::current().id());
    DriveClass::Ssd
}

#[test]
fn the_drive_probe_runs_on_the_job_thread() {
    let fs = many_files();
    let (mut browser, _) = counting_browser(&fs, 1);
    browser.set_class_of(probing);
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    let probed = PROBED_ON.lock().unwrap().clone();
    assert!(!probed.is_empty());
    assert!(!probed.contains(&std::thread::current().id()));
}
