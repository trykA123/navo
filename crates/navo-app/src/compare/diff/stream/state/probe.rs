//! Test-only views into the Stream state: queue membership, take order and held bytes.

use super::{Job, State};

impl State {
    /// True when this id waits in the focus queue; used by the focus tests.
    pub(crate) fn in_focus_queue(&self, id: u64) -> bool {
        self.scheduled.contains(&id) && self.focus_queued.contains(&id)
    }

    /// True when this id waits only in the normal queue; used by the focus tests.
    pub(crate) fn in_normal_queue(&self, id: u64) -> bool {
        self.scheduled.contains(&id) && !self.focus_queued.contains(&id)
    }

    /// The order card jobs come off the queues, draining them; used by the order tests.
    pub(crate) fn drain_order(&mut self) -> Vec<u64> {
        let mut order = Vec::new();
        while let Some(job) = self.take_job() {
            if let Job::Card(job) = job {
                order.push(job.id);
            }
        }
        order
    }

    /// Bytes of card bodies currently held.
    pub(crate) fn held_bytes(&self) -> u64 {
        self.held_bytes
    }
}
