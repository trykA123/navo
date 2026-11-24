//! Process-wide counters for the answer path; the heartbeat drains them into its log line.

use std::sync::atomic::AtomicU64;

/// Read attempts that gave up before the deadline since the last heartbeat drained the count.
pub(super) static BUSY_REPLIES: AtomicU64 = AtomicU64::new(0);
