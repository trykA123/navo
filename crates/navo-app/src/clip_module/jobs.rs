use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::lock;

type Work = Box<dyn FnOnce() + Send>;

/// Store work off the calling thread. Jobs run one at a time, in the order they were given.
#[derive(Default)]
pub(super) struct Jobs {
    last: Mutex<Option<JoinHandle<()>>>,
    pending: Arc<(Mutex<usize>, Condvar)>,
}

struct Done(Arc<(Mutex<usize>, Condvar)>);

impl Drop for Done {
    fn drop(&mut self) {
        let (count, idle) = &*self.0;
        let mut count = lock(count);
        *count = count.saturating_sub(1);
        idle.notify_all();
    }
}

impl Jobs {
    /// Starts `job` after every earlier job. When no thread can be made it runs here.
    pub(super) fn run(&self, name: &str, job: impl FnOnce() + Send + 'static) {
        *lock(&self.pending.0) += 1;
        let done = Done(self.pending.clone());
        let mut last = lock(&self.last);
        let previous = last.take();
        let work: Arc<Mutex<Option<Work>>> = Arc::new(Mutex::new(Some(Box::new(move || {
            let _done = done;
            join(previous);
            job();
        }))));
        let shared = work.clone();
        match thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || take_and_run(&shared))
        {
            Ok(handle) => *last = Some(handle),
            Err(error) => {
                drop(last);
                tracing::warn!(%error, "cannot start a clipboard thread; running the job here");
                take_and_run(&work);
            }
        }
    }

    /// Waits until every job has ended, at most `limit`. Returns false on time-out.
    pub(super) fn wait(&self, limit: Duration) -> bool {
        let (count, idle) = &*self.pending;
        let (_count, waited) = idle
            .wait_timeout_while(lock(count), limit, |count| *count > 0)
            .unwrap_or_else(PoisonError::into_inner);
        !waited.timed_out()
    }
}

fn take_and_run(work: &Mutex<Option<Work>>) {
    let job = lock(work).take();
    if let Some(job) = job {
        job();
    }
}

fn join(previous: Option<JoinHandle<()>>) {
    if let Some(previous) = previous
        && previous.join().is_err()
    {
        tracing::warn!("a clipboard job ended in a panic");
    }
}
