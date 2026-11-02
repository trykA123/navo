use std::process::Child;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use super::helper::{Helper, Prepared};
use super::pool::Pool;

const TICK: Duration = Duration::from_millis(10);
const REAP_LIMIT: Duration = Duration::from_secs(30);

enum Job {
    Dispose(Child, Arc<AtomicU64>),
    Recycle {
        helper: Helper,
        deadline: Instant,
        pool: Arc<Pool<Helper>>,
    },
    Shutdown {
        launcher: Arc<AtomicU64>,
        deadline: Instant,
        done: Sender<()>,
    },
}

struct Recycling {
    helper: Helper,
    deadline: Instant,
    pool: Arc<Pool<Helper>>,
}

struct Reaping {
    child: Child,
    launcher: Arc<AtomicU64>,
    give_up: Instant,
}

struct Shutdown {
    launcher: Arc<AtomicU64>,
    deadline: Instant,
    done: Sender<()>,
}

static WORKER: OnceLock<Option<Sender<Job>>> = OnceLock::new();

pub(super) fn dispose(child: Child, launcher: Arc<AtomicU64>) {
    if let Err(job) = submit(Job::Dispose(child, launcher))
        && let Job::Dispose(mut child, _) = *job
    {
        let _ = child.kill();
    }
}

pub(super) fn recycle(helper: Helper, timeout: Duration, pool: Arc<Pool<Helper>>) {
    let deadline = Instant::now() + timeout;
    drop(submit(Job::Recycle {
        helper,
        deadline,
        pool,
    }));
}

pub(super) fn shutdown(launcher: Arc<AtomicU64>, deadline: Instant) -> Option<Receiver<()>> {
    let worker = WORKER.get()?.as_ref()?;
    let (done, finished) = mpsc::channel();
    worker
        .send(Job::Shutdown {
            launcher,
            deadline,
            done,
        })
        .ok()?;
    Some(finished)
}

fn submit(job: Job) -> Result<(), Box<Job>> {
    let worker = WORKER.get_or_init(|| {
        let (sender, jobs) = mpsc::channel();
        thread::Builder::new()
            .name("pdf-helper-reaper".into())
            .spawn(move || run(&jobs))
            .ok()
            .map(|_| sender)
    });
    match worker {
        Some(worker) => worker.send(job).map_err(|error| Box::new(error.0)),
        None => Err(Box::new(job)),
    }
}

fn run(jobs: &Receiver<Job>) {
    let mut recycling = Vec::new();
    let mut reaping = Vec::new();
    let mut shutdowns = Vec::new();
    loop {
        let idle = recycling.is_empty() && reaping.is_empty() && shutdowns.is_empty();
        let job = if idle {
            jobs.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            jobs.recv_timeout(TICK)
        };
        match job {
            Ok(Job::Dispose(child, launcher)) => start_reaping(&mut reaping, child, launcher),
            Ok(Job::Recycle {
                mut helper,
                deadline,
                pool,
            }) => {
                if helper.begin_prepare().is_ok() {
                    recycling.push(Recycling {
                        helper,
                        deadline,
                        pool,
                    });
                } else {
                    dispose_helper(&mut reaping, helper);
                }
            }
            Ok(Job::Shutdown {
                launcher,
                deadline,
                done,
            }) => {
                stop_recycling(&mut recycling, &mut reaping, &launcher);
                shutdowns.push(Shutdown {
                    launcher,
                    deadline,
                    done,
                });
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        advance_recycling(&mut recycling, &mut reaping);
        advance_reaping(&mut reaping);
        advance_shutdowns(&mut shutdowns, &reaping);
    }
}

fn stop_recycling(
    recycling: &mut Vec<Recycling>,
    reaping: &mut Vec<Reaping>,
    launcher: &Arc<AtomicU64>,
) {
    let mut waiting = Vec::new();
    for entry in recycling.drain(..) {
        if Arc::ptr_eq(&entry.helper.shutdowns, launcher) {
            dispose_helper(reaping, entry.helper);
        } else {
            waiting.push(entry);
        }
    }
    *recycling = waiting;
}

fn advance_shutdowns(shutdowns: &mut Vec<Shutdown>, reaping: &[Reaping]) {
    shutdowns.retain(|shutdown| {
        let pending = reaping
            .iter()
            .any(|entry| Arc::ptr_eq(&entry.launcher, &shutdown.launcher));
        if pending && Instant::now() < shutdown.deadline {
            return true;
        }
        let _ = shutdown.done.send(());
        false
    });
}

fn advance_recycling(recycling: &mut Vec<Recycling>, reaping: &mut Vec<Reaping>) {
    let mut waiting = Vec::new();
    for mut entry in recycling.drain(..) {
        match entry.helper.poll_prepare() {
            Prepared::Waiting if Instant::now() < entry.deadline => waiting.push(entry),
            Prepared::Ready => entry.pool.put(entry.helper),
            Prepared::Waiting | Prepared::Failed => dispose_helper(reaping, entry.helper),
        }
    }
    *recycling = waiting;
}

fn dispose_helper(reaping: &mut Vec<Reaping>, mut helper: Helper) {
    if let Some(child) = helper.child.take() {
        start_reaping(reaping, child, helper.shutdowns.clone());
    }
}

fn start_reaping(reaping: &mut Vec<Reaping>, mut child: Child, launcher: Arc<AtomicU64>) {
    let _ = child.kill();
    reaping.push(Reaping {
        child,
        launcher,
        give_up: Instant::now() + REAP_LIMIT,
    });
}

fn advance_reaping(reaping: &mut Vec<Reaping>) {
    reaping.retain_mut(|entry| {
        matches!(entry.child.try_wait(), Ok(None)) && Instant::now() < entry.give_up
    });
}
