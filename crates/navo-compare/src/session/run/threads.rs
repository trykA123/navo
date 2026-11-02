use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::{self, JoinHandle};

use crate::model::Reason;

use super::{Context, walk};

/// Starts `body` on a new thread; false when the system refused the thread.
pub(super) fn start(
    handles: &mut Vec<JoinHandle<()>>,
    body: impl FnOnce() + Send + 'static,
) -> bool {
    match thread::Builder::new()
        .name("compare-session".to_owned())
        .spawn(body)
    {
        Ok(handle) => {
            handles.push(handle);
            true
        }
        Err(_) => false,
    }
}

pub(super) fn walk_guarded(context: &Context) {
    if catch_unwind(AssertUnwindSafe(|| walk(context))).is_err() {
        stop(context);
    }
}

pub(super) fn stop(context: &Context) {
    context.cancel.cancel();
    context.queue.close();
    context
        .shared
        .finish_problem(context.generation, Reason::WorkerPanicked);
}
