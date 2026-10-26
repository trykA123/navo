use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use super::*;

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn each_kind_has_its_own_limits() {
    assert_eq!(JobKind::Listing.limits(), Limits::LISTING);
    let preview = JobKind::Preview.limits();
    assert_eq!((preview.ssd, preview.network), (2, 2));
    assert_eq!(JobKind::Size.limits().network, 1);
    assert_eq!(JobKind::Module.limits().ssd, 1);
}

#[test]
fn the_shared_pools_are_one_instance() {
    assert!(std::ptr::eq(Pools::shared(), Pools::shared()));
}

#[test]
fn a_size_job_does_not_wait_behind_a_running_listing_on_the_same_drive() {
    let classifier: Classifier = Arc::new(|_| VolumeKind::Hdd);
    let pools = Pools::with(&classifier);
    let listing = pools.queue(JobKind::Listing);
    let size = pools.queue(JobKind::Size);
    let (listing_owner, size_owner) = (listing.owner(), size.owner());
    let release = Arc::new(Barrier::new(2));
    let (started_tx, started) = mpsc::channel();
    let held = Arc::clone(&release);
    listing.submit(
        JobSlot {
            volume: vol("V:"),
            priority: Priority::Visible,
            owner: listing_owner.id(),
        },
        Box::new(move |_| {
            started_tx.send(()).unwrap();
            held.wait();
        }),
    );
    started.recv_timeout(WAIT).unwrap();
    let (done_tx, done) = mpsc::channel();
    size.submit(
        JobSlot {
            volume: vol("V:"),
            priority: Priority::Idle,
            owner: size_owner.id(),
        },
        Box::new(move |_| done_tx.send(()).unwrap()),
    );
    assert_eq!(done.recv_timeout(WAIT), Ok(()));
    release.wait();
}
