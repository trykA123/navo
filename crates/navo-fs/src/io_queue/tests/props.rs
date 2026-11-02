use std::collections::HashSet;

use proptest::prelude::*;
use proptest::sample::Index;

use super::*;

const VOLUMES: [&str; 3] = ["C:", "D:", r"\\srv\share"];

#[derive(Debug, Clone)]
enum Op {
    Push(usize, Priority, u64),
    StartOn(usize),
    StartAny,
    Finish(Index),
    Cancel(u64),
    Reprioritise(Index, Priority),
    SetVolume(usize, VolumeKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Violation {
    OverLimit,
    NotBest,
    StartedTwice,
    CancelledStarted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Queued,
    Running,
    Done,
    Cancelled,
}

struct ModelJob {
    id: JobId,
    volume: usize,
    priority: Priority,
    owner: OwnerId,
    state: State,
}

struct Model {
    scheduler: Scheduler,
    jobs: Vec<ModelJob>,
    kinds: [VolumeKind; 3],
    started: HashSet<JobId>,
    violations: Vec<Violation>,
}

impl Model {
    fn new() -> Self {
        Self {
            scheduler: Scheduler::new(),
            jobs: Vec::new(),
            kinds: [VolumeKind::Unknown; 3],
            started: HashSet::new(),
            violations: Vec::new(),
        }
    }

    fn running(&self, volume: usize) -> usize {
        self.jobs
            .iter()
            .filter(|j| j.volume == volume && j.state == State::Running)
            .count()
    }

    fn open(&self, volume: usize) -> bool {
        self.running(volume) < limit_of(self.kinds[volume])
    }

    fn best(&self, volumes: &[usize]) -> Option<(usize, JobId)> {
        let mut best: Option<&ModelJob> = None;
        let candidates = self
            .jobs
            .iter()
            .filter(|j| j.state == State::Queued && volumes.contains(&j.volume));
        for job in candidates {
            if best.is_none_or(|b| job.priority > b.priority) {
                best = Some(job);
            }
        }
        best.map(|j| (j.volume, j.id))
    }

    fn record_start(&mut self, expected: Option<(usize, JobId)>, actual: Option<(usize, JobId)>) {
        if expected != actual {
            self.violations.push(Violation::NotBest);
        }
        let Some((_, id)) = actual else { return };
        if !self.started.insert(id) {
            self.violations.push(Violation::StartedTwice);
        }
        let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) else {
            return;
        };
        if job.state == State::Cancelled {
            self.violations.push(Violation::CancelledStarted);
        }
        job.state = State::Running;
        let volume = job.volume;
        if self.running(volume) > limit_of(self.kinds[volume]) {
            self.violations.push(Violation::OverLimit);
        }
    }

    fn start_on(&mut self, volume: usize) {
        let expected = self.open(volume).then(|| self.best(&[volume])).flatten();
        let actual = self.scheduler.start_next_on(&vol(VOLUMES[volume]));
        self.record_start(expected, actual.map(|id| (volume, id)));
    }

    fn start_any(&mut self) {
        let open: Vec<usize> = (0..VOLUMES.len()).filter(|&v| self.open(v)).collect();
        let expected = self.best(&open);
        let actual = self.scheduler.start_next().map(|(volume, id)| {
            let index = VOLUMES.iter().position(|v| *v == volume.as_str());
            (index.unwrap_or(usize::MAX), id)
        });
        self.record_start(expected, actual);
    }

    fn apply(&mut self, op: &Op) {
        match *op {
            Op::Push(volume, priority, owner) => {
                let owner = OwnerId(owner);
                let id = self.scheduler.push(vol(VOLUMES[volume]), priority, owner);
                self.jobs.push(ModelJob {
                    id,
                    volume,
                    priority,
                    owner,
                    state: State::Queued,
                });
            }
            Op::StartOn(volume) => self.start_on(volume),
            Op::StartAny => self.start_any(),
            Op::Finish(pick) => self.finish(pick),
            Op::Cancel(owner) => self.cancel(OwnerId(owner)),
            Op::Reprioritise(pick, priority) => self.reprioritise(pick, priority),
            Op::SetVolume(volume, kind) => {
                self.scheduler.set_volume(vol(VOLUMES[volume]), kind);
                self.kinds[volume] = kind;
            }
        }
    }

    fn finish(&mut self, pick: Index) {
        if self.jobs.is_empty() {
            return;
        }
        let job = pick.get_mut(&mut self.jobs);
        self.scheduler.finish(job.id);
        if job.state == State::Running {
            job.state = State::Done;
        }
    }

    fn cancel(&mut self, owner: OwnerId) {
        self.scheduler.cancel_owner(owner);
        for job in self.jobs.iter_mut().filter(|j| j.owner == owner) {
            if job.state == State::Queued {
                job.state = State::Cancelled;
            }
        }
    }

    fn reprioritise(&mut self, pick: Index, priority: Priority) {
        if self.jobs.is_empty() {
            return;
        }
        let job = pick.get_mut(&mut self.jobs);
        self.scheduler.reprioritise(job.id, priority);
        if job.state == State::Queued {
            job.priority = priority;
        }
    }
}

fn run(ops: &[Op]) -> Vec<Violation> {
    let mut model = Model::new();
    for op in ops {
        model.apply(op);
    }
    model.violations
}

fn priority() -> impl Strategy<Value = Priority> {
    prop_oneof![
        Just(Priority::Idle),
        Just(Priority::Prefetch),
        Just(Priority::Visible)
    ]
}

fn kind() -> impl Strategy<Value = VolumeKind> {
    prop_oneof![
        Just(VolumeKind::Ssd),
        Just(VolumeKind::Hdd),
        Just(VolumeKind::Network),
        Just(VolumeKind::Removable),
        Just(VolumeKind::Unknown)
    ]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..3usize, priority(), 0..3u64).prop_map(|(v, p, o)| Op::Push(v, p, o)),
        3 => (0..3usize).prop_map(Op::StartOn),
        3 => Just(Op::StartAny),
        3 => any::<Index>().prop_map(Op::Finish),
        1 => (0..3u64).prop_map(Op::Cancel),
        1 => (any::<Index>(), priority()).prop_map(|(i, p)| Op::Reprioritise(i, p)),
        1 => (0..3usize, kind()).prop_map(|(v, k)| Op::SetVolume(v, k)),
    ]
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(op(), 0..80)
}

proptest! {
    #[test]
    fn running_never_exceeds_the_limit(ops in ops()) {
        prop_assert!(!run(&ops).contains(&Violation::OverLimit));
    }

    #[test]
    fn every_start_takes_the_best_queued_job(ops in ops()) {
        prop_assert!(!run(&ops).contains(&Violation::NotBest));
    }

    #[test]
    fn no_job_starts_twice(ops in ops()) {
        prop_assert!(!run(&ops).contains(&Violation::StartedTwice));
    }

    #[test]
    fn cancelled_queued_jobs_never_start(ops in ops()) {
        prop_assert!(!run(&ops).contains(&Violation::CancelledStarted));
    }
}
