use rusty_core::{ConflictPolicy, FsError, Location, OperationPlan, PlanRequest, plan};
use rusty_fs::{
    AutoTuning, CancelToken, CopyMode, DriveClass, JobContext, JobOutcome, Progress, ProviderOps,
    WorkerCounts, existing_at, job_workers, run_job_parallel, scan,
};

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use super::{Copying, Ended, JobUpdate, LogFactory, OpRequest};
use crate::browser::{
    Message, RenameReport, UndoLogFactory, UndoOffer, Waker, run_batch, run_undo,
};

const PROGRESS_EVERY: Duration = Duration::from_millis(50);

pub(super) fn lands_inside(target: &Location, folder: &Location, sources: &[Location]) -> bool {
    if target == folder {
        return true;
    }
    let mut current = Some(target.clone());
    while let Some(location) = current {
        if sources.contains(&location) {
            return true;
        }
        current = location.parent();
    }
    false
}

pub(super) fn start_progress(plan: &OperationPlan) -> Progress {
    Progress {
        bytes_done: 0,
        bytes_total: plan.totals.bytes,
        files_done: 0,
        files_total: plan.totals.files,
        current: None,
        paused: false,
    }
}

pub(super) fn plan_for(
    provider: &dyn ProviderOps,
    request: &OpRequest,
    cancel: &CancelToken,
) -> Result<OperationPlan, FsError> {
    let items = scan(provider, &request.sources, cancel)?;
    let existing = match &request.target {
        Some(target) => existing_at(provider, &items, target, cancel)?,
        None => Vec::new(),
    };
    let same_volume = if request.same_volume
        && let Some(source) = request.sources.first().and_then(Location::parent)
        && let Some(target) = &request.target
    {
        match (
            provider.volume_serial(&source).ok().flatten(),
            provider.volume_serial(target).ok().flatten(),
        ) {
            (Some(source), Some(target)) => source == target,
            _ => true,
        }
    } else {
        request.same_volume
    };
    Ok(plan(&PlanRequest {
        kind: request.kind,
        items,
        target: request.target.clone(),
        existing,
        policy: ConflictPolicy::Ask,
        same_volume,
        protected: request.protected.clone(),
    }))
}

pub(super) fn worker_count(
    mode: CopyMode,
    source: Option<&Location>,
    target: Option<&Location>,
    class_of: impl Fn(&Location) -> DriveClass,
) -> usize {
    let class = |place: Option<&Location>| place.map_or(DriveClass::Unknown, &class_of);
    let counts = WorkerCounts::from(&AutoTuning::default());
    job_workers(mode, class(source), class(target), counts)
}

pub(in crate::browser) enum Run {
    Plain,
    Rename,
    Undo {
        offer: UndoOffer,
        log: UndoLogFactory,
    },
}

pub(super) struct JobWork {
    pub id: u64,
    pub copying: Copying,
    pub source: Option<Location>,
    pub target: Option<Location>,
    pub plan: OperationPlan,
    pub ops: Arc<dyn ProviderOps>,
    pub factory: LogFactory,
    pub run: Run,
    pub cancel: CancelToken,
    pub sender: Sender<Message>,
    pub wake: Waker,
}

struct Reporter<'a> {
    id: u64,
    sender: &'a Sender<Message>,
    wake: &'a Waker,
    sent: Instant,
    last: Option<Progress>,
}

impl Reporter<'_> {
    fn report(&mut self, progress: &Progress) {
        let was_paused = self.last.as_ref().is_some_and(|last| last.paused);
        self.last = Some(progress.clone());
        if progress.paused == was_paused && self.sent.elapsed() < PROGRESS_EVERY {
            return;
        }
        self.sent = Instant::now();
        let progress = progress.clone();
        let id = self.id;
        let _ = self
            .sender
            .send(Message::Job(JobUpdate::Progress { id, progress }));
        (self.wake)();
    }
}

impl JobWork {
    pub(super) fn run(self) {
        let mut reporter = Reporter {
            id: self.id,
            sender: &self.sender,
            wake: &self.wake,
            sent: Instant::now(),
            last: None,
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.execute(&mut |progress| reporter.report(progress))
        }))
        .unwrap_or_else(|_| Err("The job stopped unexpectedly".to_owned()));
        let (outcome, report) = match outcome {
            Ok((outcome, report)) => (Ok(outcome), report),
            Err(message) => (Err(message), RenameReport::default()),
        };
        let _ = self.sender.send(Message::Job(JobUpdate::Done {
            id: self.id,
            ended: Ended {
                progress: reporter.last,
                outcome,
                report,
            },
        }));
        (self.wake)();
    }

    fn execute(
        &self,
        progress: &mut dyn FnMut(&Progress),
    ) -> Result<(JobOutcome, RenameReport), String> {
        let mut log = match &self.run {
            Run::Undo { offer, log } => log(offer)?,
            Run::Plain | Run::Rename => (self.factory)(&self.plan)?,
        };
        let mut ctx = JobContext {
            ops: self.ops.as_ref(),
            log: log.as_mut(),
            cancel: &self.cancel,
            progress,
        };
        let (outcome, report) = match &self.run {
            Run::Plain => (self.run_plain(&mut ctx), RenameReport::default()),
            Run::Rename => run_batch(&self.plan, &mut ctx),
            Run::Undo { offer, .. } => run_undo(&offer.plan, &offer.done, &mut ctx),
        };
        log.finish(&outcome, self.ops.as_ref());
        Ok((outcome, report))
    }

    fn run_plain(&self, ctx: &mut JobContext<'_>) -> JobOutcome {
        let Copying { mode, class_of } = self.copying;
        let workers = worker_count(mode, self.source.as_ref(), self.target.as_ref(), class_of);
        run_job_parallel(&self.plan, &[], ctx, workers)
    }
}

#[cfg(test)]
mod tests {
    use rusty_core::{AppState, OpKind, Step};
    use rusty_fs::FakeFs;

    use super::*;

    fn move_plan(source_serial: Option<u64>, target_serial: Option<u64>) -> OperationPlan {
        let fs = FakeFs::new();
        fs.add_file("home/a.txt", 3);
        fs.add_folder("dest");
        fs.set_volume_serial("home", source_serial);
        fs.set_volume_serial("dest", target_serial);
        fs.set_volume("home/a.txt", 2);
        let state = AppState::new(fs.location("home"));
        let request = OpRequest {
            id: 1,
            kind: OpKind::Move,
            sources: vec![fs.location("home/a.txt")],
            target: Some(fs.location("dest")),
            same_volume: true,
            protected: Vec::new(),
            source_tab: state.active_tab().id(),
            asks: true,
        };
        plan_for(&fs, &request, &CancelToken::new()).unwrap()
    }

    #[test]
    fn differing_volume_serials_copy_then_remove() {
        let plan = move_plan(Some(1), Some(2));
        assert!(matches!(
            plan.steps.as_slice(),
            [Step::CopyFile { .. }, Step::RemoveFile { .. }]
        ));
    }

    #[test]
    fn equal_volume_serials_rename() {
        let plan = move_plan(Some(1), Some(1));
        assert!(matches!(plan.steps.as_slice(), [Step::Rename { .. }]));
    }

    #[test]
    fn unknown_source_volume_keeps_rename() {
        let plan = move_plan(None, Some(2));
        assert!(matches!(plan.steps.as_slice(), [Step::Rename { .. }]));
    }

    #[test]
    fn unknown_target_volume_keeps_rename() {
        let plan = move_plan(Some(2), None);
        assert!(matches!(plan.steps.as_slice(), [Step::Rename { .. }]));
    }

    #[cfg(target_os = "linux")]
    fn cross_device_dirs() -> Option<(tempfile::TempDir, tempfile::TempDir)> {
        use std::os::unix::fs::MetadataExt;

        if !std::path::Path::new("/dev/shm").is_dir() {
            return None;
        }
        let source = tempfile::tempdir_in("/dev/shm").unwrap();
        let target = tempfile::tempdir().unwrap();
        (std::fs::metadata(source.path()).unwrap().dev()
            != std::fs::metadata(target.path()).unwrap().dev())
        .then_some((source, target))
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_tmpfs_move_across_devices_succeeds() {
        use std::fs;
        use std::os::unix::ffi::OsStrExt;
        use std::path::Path;
        use std::sync::mpsc;

        use rusty_core::ProviderPath;
        use rusty_fs::LocalFs;

        use crate::browser::{Browser, JobStatus};

        let Some((source_dir, target_dir)) = cross_device_dirs() else {
            return;
        };
        let source_path = source_dir.path().join("a.txt");
        fs::write(&source_path, b"cross-device move").unwrap();
        let location = |path: &Path| {
            Location::local(ProviderPath::from_bytes(path.as_os_str().as_bytes())).unwrap()
        };
        let (wake, awake) = mpsc::channel();
        let mut browser = Browser::new(
            AppState::new(location(target_dir.path())),
            Arc::new(LocalFs::new()),
            Arc::new(move || {
                let _ = wake.send(());
            }),
        );
        browser
            .paste(OpKind::Move, vec![location(&source_path)])
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            browser.poll();
            if browser
                .jobs()
                .first()
                .is_some_and(|job| job.status != JobStatus::Running)
            {
                break;
            }
            awake
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        }
        assert_eq!(browser.jobs()[0].status, JobStatus::Done);
        assert_eq!(
            fs::read(target_dir.path().join("a.txt")).unwrap(),
            b"cross-device move"
        );
        assert!(!source_path.exists());
    }
}
