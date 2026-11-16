use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::fake::FakeSource;
use super::{batches_with_row, collect, options};
use crate::model::{Reason, Status};
use crate::session::CheckMode;
use crate::session::Session;
use crate::session::run::SettleHook;

struct Seam {
    after_complete: bool,
    id: Arc<AtomicU64>,
    release: Arc<AtomicBool>,
}

impl Seam {
    fn new(after_complete: bool) -> Self {
        Self {
            after_complete,
            id: Arc::new(AtomicU64::new(0)),
            release: Arc::new(AtomicBool::new(false)),
        }
    }

    fn hook(&self) -> SettleHook {
        let after_complete = self.after_complete;
        let id = Arc::clone(&self.id);
        let release = Arc::clone(&self.release);
        Arc::new(move |phase, job_id| {
            if phase == after_complete {
                id.store(job_id.saturating_add(1), Ordering::Release);
                wait_for_release(&release);
            }
        })
    }

    fn wait(&self) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let id = self.id.load(Ordering::Acquire);
            if id != 0 {
                return id - 1;
            }
            assert!(Instant::now() < deadline, "settle seam was not reached");
            thread::yield_now();
        }
    }

    fn release(&self) {
        self.release.store(true, Ordering::Release);
    }

    fn guard(&self) -> ReleaseGuard {
        ReleaseGuard {
            release: Arc::clone(&self.release),
        }
    }
}

fn wait_for_release(release: &AtomicBool) {
    while !release.load(Ordering::Acquire) {
        thread::yield_now();
    }
}

struct ReleaseGuard {
    release: Arc<AtomicBool>,
}

impl Drop for ReleaseGuard {
    fn drop(&mut self) {
        self.release.store(true, Ordering::Release);
    }
}

impl Drop for Seam {
    fn drop(&mut self) {
        self.release();
    }
}

fn session_with_seam(seam: &Seam) -> Session {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("a.bin", vec![0xAA; 1024]);
    right.add_file("a.bin", vec![0xAA; 1024]);
    Session::start_with_settle_hook(
        Arc::new(left),
        Arc::new(right),
        options(CheckMode::Content),
        seam.hook(),
    )
}

fn first_id(session: &Session) -> u64 {
    let first = session
        .batches()
        .recv_timeout(Duration::from_secs(20))
        .expect("initial batch");
    first.rows[0].id
}

#[test]
fn a_stop_before_complete_settles_not_checked() {
    let seam = Seam::new(false);
    let session = session_with_seam(&seam);
    let _guard = seam.guard();
    let id = first_id(&session);
    assert_eq!(seam.wait(), id);
    assert!(session.stop_row(id));
    seam.release();

    let batches = collect(&session);
    let final_batch = batches.last().expect("final batch");
    assert_eq!(final_batch.totals.not_checked, 1);
    let row = batches_with_row(&batches, "a.bin").last().expect("row");
    assert_eq!(row.1.id, id);
    assert_eq!(row.1.raw_status, Status::NotChecked(Reason::Stopped));
}

#[test]
fn a_stop_after_complete_keeps_the_check_result() {
    let seam = Seam::new(true);
    let session = session_with_seam(&seam);
    let _guard = seam.guard();
    let id = first_id(&session);
    assert_eq!(seam.wait(), id);
    assert!(!session.stop_row(id));
    seam.release();

    let batches = collect(&session);
    let final_batch = batches.last().expect("final batch");
    assert_eq!(final_batch.totals.not_checked, 0);
    assert_eq!(final_batch.totals.same, 1);
    let row = batches_with_row(&batches, "a.bin").last().expect("row");
    assert_eq!(row.1.id, id);
    assert_eq!(row.1.raw_status, Status::Same);
}
