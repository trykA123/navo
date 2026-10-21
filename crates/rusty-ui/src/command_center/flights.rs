use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_fs::{VolumeId, volume_of};

pub const RETRY_AFTER: Duration = Duration::from_secs(30);
pub const THREAD_CAP: usize = 4;
const POLL: Duration = Duration::from_millis(20);

pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub enum Answer<T> {
    Ready(T),
    Unreachable,
    Busy(Location),
    Full,
    Cancelled,
}

pub struct Ask<'a> {
    pub key: &'a Location,
    pub since: Instant,
    pub cancel: &'a dyn Fn() -> bool,
}

#[derive(Clone)]
pub struct Rules<T> {
    pub name: &'static str,
    pub wait: Duration,
    pub is_down: fn(&T) -> bool,
    pub now: Clock,
}

struct Done<T> {
    value: Option<T>,
    finished: Option<Instant>,
    down_since: Option<Instant>,
    timed_out: bool,
    is_gone: bool,
}

struct Job<T> {
    key: Location,
    root: VolumeId,
    deadline: Instant,
    done: Mutex<Done<T>>,
    changed: Condvar,
}

impl<T: Clone> Job<T> {
    fn lock(&self) -> MutexGuard<'_, Done<T>> {
        self.done.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_running(&self) -> bool {
        let done = self.lock();
        done.value.is_none() && !done.is_gone
    }

    fn abandon(&self) {
        let mut done = self.lock();
        if done.value.is_none() {
            done.is_gone = true;
            self.changed.notify_all();
        }
    }

    fn finish(&self, value: T, now: Instant, is_down: bool) {
        let mut done = self.lock();
        done.down_since = is_down.then(|| done.down_since.unwrap_or(now));
        done.value = Some(value);
        done.finished = Some(now);
        self.changed.notify_all();
    }

    fn time_out(&self, now: Instant) {
        let mut done = self.lock();
        if done.value.is_none() {
            done.timed_out = true;
            done.down_since.get_or_insert(now);
        }
    }

    fn answer(&self, ask: &Ask<'_>, now: Instant) -> Option<Step<T>> {
        let done = self.lock();
        let is_down = done.down_since.is_some_and(|at| fresh(at, now));
        match &done.value {
            None if done.is_gone => None,
            None if done.timed_out => Some(Step::Answer(Answer::Unreachable)),
            None => Some(Step::Wait),
            Some(value) if is_down || done.finished.is_some_and(|at| at >= ask.since) => {
                Some(Step::Answer(Answer::Ready(value.clone())))
            }
            Some(_) => None,
        }
    }

    fn is_unreachable(&self, now: Instant) -> bool {
        let done = self.lock();
        match done.value {
            None => done.timed_out && !done.is_gone,
            Some(_) => done.down_since.is_some_and(|at| fresh(at, now)),
        }
    }

    fn is_kept(&self, now: Instant) -> bool {
        let done = self.lock();
        (done.value.is_none() && !done.is_gone) || done.finished.is_some_and(|at| fresh(at, now))
    }
}

fn fresh(at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(at) < RETRY_AFTER
}

struct Gone<T: Clone>(Arc<Job<T>>, Arc<Inner<T>>);

impl<T: Clone> Drop for Gone<T> {
    fn drop(&mut self) {
        self.0.abandon();
        let _jobs = self.1.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        self.1.room.notify_all();
    }
}

enum Next<T> {
    Answer(Answer<T>),
    Join(Arc<Job<T>>, bool),
    Full,
}

enum Step<T> {
    Answer(Answer<T>),
    Wait,
}

struct Inner<T> {
    rules: Rules<T>,
    jobs: Mutex<Vec<Arc<Job<T>>>>,
    room: Condvar,
}

pub struct Flights<T>(Arc<Inner<T>>);

impl<T> Clone for Flights<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Clone + Send + 'static> Flights<T> {
    #[must_use]
    pub fn new(rules: Rules<T>) -> Self {
        Self(Arc::new(Inner {
            rules,
            jobs: Mutex::new(Vec::new()),
            room: Condvar::new(),
        }))
    }

    #[must_use]
    pub fn now(&self) -> Instant {
        (self.0.rules.now)()
    }

    fn jobs(&self) -> MutexGuard<'_, Vec<Arc<Job<T>>>> {
        self.0.jobs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn is_unreachable(&self, key: &Location) -> bool {
        let now = self.now();
        self.jobs()
            .iter()
            .any(|job| job.key.same_place(key) && job.is_unreachable(now))
    }

    pub fn get(&self, ask: &Ask<'_>, run: impl FnOnce() -> T + Send + 'static) -> Answer<T> {
        let (mut run, deadline) = (Some(run), Instant::now() + self.0.rules.wait);
        loop {
            let answer = match self.step(ask, &mut run) {
                Next::Answer(answer) => Some(answer),
                Next::Join(job, own) => self.wait_on(&job, own, ask),
                Next::Full => self.wait_for_room(deadline, ask),
            };
            if let Some(answer) = answer {
                return answer;
            }
        }
    }

    fn wait_for_room(&self, deadline: Instant, ask: &Ask<'_>) -> Option<Answer<T>> {
        let mut jobs = self.jobs();
        while jobs.iter().filter(|job| job.is_running()).count() >= THREAD_CAP {
            if (ask.cancel)() {
                return Some(Answer::Cancelled);
            }
            if Instant::now() >= deadline {
                return Some(Answer::Full);
            }
            jobs = self
                .0
                .room
                .wait_timeout(jobs, POLL)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        None
    }

    fn step(
        &self,
        ask: &Ask<'_>,
        run: &mut Option<impl FnOnce() -> T + Send + 'static>,
    ) -> Next<T> {
        let now = self.now();
        let mut jobs = self.jobs();
        jobs.retain(|job| job.is_kept(now));
        if let Some(at) = jobs.iter().position(|job| job.key.same_place(ask.key)) {
            match jobs[at].answer(ask, now) {
                Some(Step::Answer(answer)) => return Next::Answer(answer),
                Some(Step::Wait) => return Next::Join(jobs[at].clone(), true),
                None => drop(jobs.remove(at)),
            }
        }
        let root = volume_of(ask.key);
        let running = |job: &&Arc<Job<T>>| job.is_running();
        if let Some(other) = jobs.iter().filter(running).find(|job| job.root == root) {
            if other.lock().timed_out {
                return Next::Answer(Answer::Busy(other.key.clone()));
            }
            return Next::Join(other.clone(), false);
        }
        if jobs.iter().filter(running).count() >= THREAD_CAP {
            return Next::Full;
        }
        let job = run.take().and_then(|run| self.spawn(ask.key, root, run));
        let Some(job) = job else {
            return Next::Answer(Answer::Cancelled);
        };
        jobs.push(job.clone());
        Next::Join(job, true)
    }

    fn spawn(
        &self,
        key: &Location,
        root: VolumeId,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<Arc<Job<T>>> {
        let job = Arc::new(Job {
            key: key.clone(),
            root,
            deadline: Instant::now() + self.0.rules.wait,
            done: Mutex::new(Done {
                value: None,
                finished: None,
                down_since: None,
                timed_out: false,
                is_gone: false,
            }),
            changed: Condvar::new(),
        });
        let (worker, rules, inner) = (job.clone(), self.0.rules.clone(), self.0.clone());
        thread::Builder::new()
            .name(rules.name.into())
            .spawn(move || {
                let _gone = Gone(worker.clone(), inner);
                let value = run();
                let is_down = (rules.is_down)(&value);
                worker.finish(value, (rules.now)(), is_down);
            })
            .ok()?;
        Some(job)
    }

    fn give_up(&self, job: &Job<T>, own: bool) -> Answer<T> {
        job.time_out(self.now());
        if own {
            Answer::Unreachable
        } else {
            Answer::Busy(job.key.clone())
        }
    }

    fn wait_on(&self, job: &Job<T>, own: bool, ask: &Ask<'_>) -> Option<Answer<T>> {
        let mut done = job.lock();
        loop {
            if let Some(value) = &done.value {
                return own.then(|| Answer::Ready(value.clone()));
            }
            if done.is_gone {
                return own.then_some(Answer::Cancelled);
            }
            if (ask.cancel)() {
                return Some(Answer::Cancelled);
            }
            if Instant::now() >= job.deadline {
                drop(done);
                return Some(self.give_up(job, own));
            }
            done = job
                .changed
                .wait_timeout(done, POLL)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}
