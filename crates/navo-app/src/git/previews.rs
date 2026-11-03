use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

use navo_core::Location;
use navo_store::GitSettings;
use navo_ui::git::{GitAction, Previewed};

use super::hooks::lock;
use super::wiring::Tools;

pub(super) type Done = Box<dyn FnOnce(Previewed) + Send>;

struct Waiting {
    repo: Location,
    done: Vec<Done>,
}

type Queue = Arc<Mutex<Vec<Waiting>>>;

/// One worker thread that reads Pull and Push confirmations; requests for one repository share a read.
pub(super) struct Previews {
    jobs: Option<Sender<(Location, GitSettings)>>,
    waiting: Queue,
}

impl Previews {
    pub(super) fn start(tools: Arc<Tools>) -> Self {
        let waiting = Queue::default();
        let (jobs, inbox) = channel();
        let worker = Worker {
            tools,
            waiting: waiting.clone(),
            inbox,
        };
        let started = thread::Builder::new()
            .name("navo-git-previews".to_owned())
            .spawn(move || worker.run());
        if let Err(error) = &started {
            tracing::warn!(%error, "the Git preview thread did not start");
        }
        Self {
            jobs: started.ok().map(|_| jobs),
            waiting,
        }
    }

    /// Queues the read, or joins the one already queued or running for `repo`.
    pub(super) fn request(&self, repo: &Location, settings: GitSettings, done: Done) {
        let Some(jobs) = &self.jobs else {
            return done(Vec::new());
        };
        {
            let mut waiting = lock(&self.waiting);
            if let Some(held) = waiting.iter_mut().find(|held| held.repo == *repo) {
                held.done.push(done);
                return;
            }
            waiting.push(Waiting {
                repo: repo.clone(),
                done: vec![done],
            });
        }
        if jobs.send((repo.clone(), settings)).is_err() {
            finish(&self.waiting, repo, &Vec::new());
        }
    }
}

fn finish(waiting: &Queue, repo: &Location, replies: &Previewed) {
    let mut queue = lock(waiting);
    let at = queue.iter().position(|held| held.repo == *repo);
    let done = at.map(|at| queue.remove(at).done).unwrap_or_default();
    drop(queue);
    for done in done {
        done(replies.clone());
    }
}

struct Worker {
    tools: Arc<Tools>,
    waiting: Queue,
    inbox: Receiver<(Location, GitSettings)>,
}

impl Worker {
    fn run(self) {
        for (repo, settings) in &self.inbox {
            let read = catch_unwind(AssertUnwindSafe(|| self.tools.previews(&repo, &settings)));
            let replies = read.unwrap_or_else(|_| {
                tracing::warn!("a Git preview job panicked; its menu gets no confirmation");
                vec![(GitAction::Pull, None), (GitAction::Push, None)]
            });
            finish(&self.waiting, &repo, &replies);
        }
    }
}
