use crate::DriveClass::{Hdd, Network, Ssd, Unknown};
use crate::{DriveClass, WorkerCounts, copy_workers};

const COUNTS: WorkerCounts = WorkerCounts {
    ssd: 4,
    network: 8,
    slow: 1,
};

#[test]
fn the_worker_count_follows_the_slower_side_of_the_copy() {
    let table: [(DriveClass, DriveClass, usize); 16] = [
        (Ssd, Ssd, 4),
        (Ssd, Hdd, 1),
        (Ssd, Network, 8),
        (Ssd, Unknown, 1),
        (Hdd, Ssd, 1),
        (Hdd, Hdd, 1),
        (Hdd, Network, 1),
        (Hdd, Unknown, 1),
        (Network, Ssd, 8),
        (Network, Hdd, 1),
        (Network, Network, 8),
        (Network, Unknown, 1),
        (Unknown, Ssd, 1),
        (Unknown, Hdd, 1),
        (Unknown, Network, 1),
        (Unknown, Unknown, 1),
    ];
    for (source, target, workers) in table {
        assert_eq!(
            copy_workers(source, target, COUNTS),
            workers,
            "{source:?} to {target:?}"
        );
    }
}

#[test]
fn the_worker_counts_come_from_the_caller() {
    let counts = WorkerCounts {
        ssd: 6,
        network: 12,
        slow: 2,
    };
    assert_eq!(
        [
            copy_workers(Ssd, Ssd, counts),
            copy_workers(Network, Ssd, counts),
            copy_workers(Hdd, Ssd, counts),
        ],
        [6, 12, 2]
    );
}

#[test]
fn a_zero_count_still_copies_with_one_worker() {
    let counts = WorkerCounts {
        ssd: 0,
        network: 0,
        slow: 0,
    };
    assert_eq!(copy_workers(Ssd, Ssd, counts), 1);
}
