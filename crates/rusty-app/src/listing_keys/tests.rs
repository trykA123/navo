use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rusty_cache::{DriveIdentity, DriveProbe};

use super::remembered;

fn counting(calls: &Arc<AtomicUsize>) -> DriveProbe {
    let calls = Arc::clone(calls);
    Arc::new(move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        Some(DriveIdentity::Serial(7))
    })
}

#[test]
fn a_drive_is_probed_once_while_its_answer_is_fresh() {
    let calls = Arc::new(AtomicUsize::new(0));
    let probe = remembered(counting(&calls), Duration::from_secs(30));
    assert_eq!(probe('c'), Some(DriveIdentity::Serial(7)));
    assert_eq!(probe('C'), Some(DriveIdentity::Serial(7)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_stale_answer_is_probed_again() {
    let calls = Arc::new(AtomicUsize::new(0));
    let probe = remembered(counting(&calls), Duration::ZERO);
    let _ = probe('C');
    let _ = probe('C');
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn the_app_keys_fill_a_volume_serial_for_local_folders() {
    let key = super::cache_keys().key(&rusty_core::Location::new(r"C:\Users").unwrap());
    assert!(key.volume_serial.is_some());
}
