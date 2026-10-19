use rusty_fs::{CopyMode, DriveClass};

use super::*;
use crate::browser::operate::worker_count;

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

#[test]
fn a_copy_ends_the_same_in_both_modes() {
    for mode in [CopyMode::Auto, CopyMode::Standard] {
        let fs = fake();
        let mut browser = browser(&fs, home_beside_dest(&fs));
        browser.set_copy_mode(mode);
        browser.prepare(OpKind::Copy);
        settle(&mut browser);
        browser.confirm();
        settle(&mut browser);
        assert!(exists(&fs, "dest/docs/report.txt"), "{mode:?}");
    }
}
