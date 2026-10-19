use crate::DriveClass::{Hdd, Network, Ssd, Unknown};
use crate::{CopyMode, DriveClass, WorkerCounts, copy_workers, job_workers};

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

#[test]
fn standard_mode_always_runs_one_worker() {
    for (source, target) in [(Ssd, Ssd), (Network, Network), (Ssd, Network)] {
        assert_eq!(job_workers(CopyMode::Standard, source, target, COUNTS), 1);
    }
}

#[test]
fn auto_mode_follows_the_drives() {
    assert_eq!(job_workers(CopyMode::Auto, Ssd, Ssd, COUNTS), 4);
    assert_eq!(job_workers(CopyMode::Auto, Network, Ssd, COUNTS), 8);
    assert_eq!(job_workers(CopyMode::Auto, Hdd, Ssd, COUNTS), 1);
}

#[test]
fn the_mode_names_itself_for_the_dialog() {
    assert_eq!(CopyMode::default().label(), "Auto");
    assert_eq!(CopyMode::Standard.label(), "Standard");
}
