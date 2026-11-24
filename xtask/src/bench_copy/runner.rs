use std::{
    path::Path,
    time::{Duration, Instant},
};

use navo_core::{
    ConflictPolicy, FsError, Location, OpKind, OperationPlan, PlanRequest, Step, plan,
};
use navo_fs::{
    AutoTuning, CancelToken, HeldFile, JobContext, JobLog, JobOutcome, LocalFs, StepOutcome,
    WorkerCounts, copy_workers, drive_class, existing_at, run_job, run_job_parallel, scan,
};

use super::{
    fixture,
    options::Options,
    report::{self, Measurement},
    workspace::Workspace,
};

#[derive(Clone, Copy)]
enum Method {
    Standard,
    Auto,
    #[cfg(windows)]
    Robocopy,
}

impl Method {
    fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Auto => "Auto",
            #[cfg(windows)]
            Self::Robocopy => "robocopy",
        }
    }
}

struct Benchmark<'a> {
    workspace: &'a Workspace,
    ops: LocalFs,
    workers: usize,
    runs: usize,
}

struct Samples {
    rows: Vec<Measurement>,
    steps_lines: Vec<String>,
}

pub(super) fn measure(options: &Options, workspace: &Workspace) -> Result<String, String> {
    let classes = [
        drive_class(&location(&workspace.source)?),
        drive_class(&location(&workspace.target)?),
    ];
    let tuning = AutoTuning::default();
    let workers = copy_workers(
        classes[0],
        classes[1],
        WorkerCounts {
            ssd: tuning.ssd_workers,
            network: tuning.network_workers,
            slow: tuning.slow_workers,
        },
    );
    let benchmark = Benchmark {
        workspace,
        ops: LocalFs::with_auto_tuning(&tuning),
        workers,
        runs: options.runs,
    };
    let methods = [
        Method::Standard,
        Method::Auto,
        #[cfg(windows)]
        Method::Robocopy,
    ];
    let mut samples = Samples {
        rows: Vec::new(),
        steps_lines: Vec::new(),
    };
    for &set in &options.sets {
        benchmark.measure_set(set, &methods, &mut samples)?;
    }
    let mut output = report::markdown(&samples.rows, classes, workers, options.runs);
    for line in samples.steps_lines {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}

impl Benchmark<'_> {
    /// Generates one set and measures it, collecting rows or the steps line.
    fn measure_set(
        &self,
        set: fixture::DataSet,
        methods: &[Method],
        samples: &mut Samples,
    ) -> Result<(), String> {
        let source = self.workspace.source.join(set.name);
        eprintln!(
            "bench-copy: generating {} ({} bytes)",
            set.name,
            set.bytes()
        );
        fixture::generate(&source, set)?;
        if set == fixture::STEPS {
            samples.steps_lines.push(self.measure_steps(&source)?);
            return Ok(());
        }
        let plan = build_plan(&self.ops, &source, &self.workspace.target)?;
        for &method in methods {
            let seconds = self.measure_method(&plan, method, self.runs)?;
            samples.rows.push(Measurement {
                set,
                method: method.name(),
                seconds,
            });
        }
        Ok(())
    }

    fn measure_method(
        &self,
        plan: &OperationPlan,
        method: Method,
        runs: usize,
    ) -> Result<Vec<f64>, String> {
        let mut samples = Vec::new();
        for run in 1..=runs {
            eprintln!("bench-copy: {} run {run}/{runs}", method.name());
            let started = Instant::now();
            match method {
                Method::Standard | Method::Auto => self.run_copy(plan, method)?,
                #[cfg(windows)]
                Method::Robocopy => robocopy(plan, self.workspace)?,
            }
            samples.push(started.elapsed().as_secs_f64());
            self.workspace.reset_target()?;
        }
        Ok(samples)
    }

    fn run_copy(&self, plan: &OperationPlan, method: Method) -> Result<(), String> {
        let cancel = CancelToken::new();
        let mut log = BenchLog;
        let mut progress = |_: &navo_fs::Progress| {};
        let mut context = JobContext {
            ops: &self.ops,
            log: &mut log,
            cancel: &cancel,
            progress: &mut progress,
        };
        let outcome = match method {
            Method::Standard => run_job(plan, &[], &mut context),
            _ => run_job_parallel(plan, &[], &mut context, self.workers),
        };
        match outcome {
            JobOutcome::Done => Ok(()),
            outcome => Err(format!("{} copy failed: {outcome:?}", method.name())),
        }
    }

    fn measure_steps(&self, source: &Path) -> Result<String, String> {
        let plan = build_plan(&self.ops, source, &self.workspace.target)?;
        let copy_total = plan
            .steps
            .iter()
            .filter(|step| matches!(step, Step::CopyFile { .. }))
            .count() as u64;
        if copy_total != fixture::STEPS.files {
            return Err(format!(
                "steps plan has {copy_total} copy steps, expected {}",
                fixture::STEPS.files
            ));
        }
        eprintln!("bench-copy: steps run 1/1");
        let cancel = CancelToken::new();
        let mut log = StepLog::new(&plan);
        let mut progress = |_: &navo_fs::Progress| {};
        let mut context = JobContext {
            ops: &self.ops,
            log: &mut log,
            cancel: &cancel,
            progress: &mut progress,
        };
        match run_job(&plan, &[], &mut context) {
            JobOutcome::Done => {}
            outcome => return Err(format!("steps copy failed: {outcome:?}")),
        }
        let line = log.line()?;
        self.workspace.reset_target()?;
        Ok(line)
    }
}

fn build_plan(ops: &LocalFs, source: &Path, target: &Path) -> Result<OperationPlan, String> {
    let cancel = CancelToken::new();
    let items = scan(ops, &[location(source)?], &cancel).map_err(|error| error.message())?;
    let target = location(target)?;
    let existing = existing_at(ops, &items, &target, &cancel).map_err(|error| error.message())?;
    let plan = plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(target),
        existing,
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    });
    if !plan.conflicts.is_empty() {
        return Err("unexpected copy plan conflicts".to_owned());
    }
    Ok(plan)
}

struct BenchLog;

impl JobLog for BenchLog {
    fn step(&mut self, _seq: u32, _outcome: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn undone(&mut self, _seq: u32, _outcome: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, _held: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

fn location(path: &Path) -> Result<Location, String> {
    #[cfg(unix)]
    let provider_path = {
        use std::os::unix::ffi::OsStrExt;
        navo_core::ProviderPath::from_bytes(path.as_os_str().as_bytes())
    };
    #[cfg(windows)]
    let provider_path = {
        use std::os::windows::ffi::OsStrExt;
        navo_core::ProviderPath::from_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
    };
    Location::local(provider_path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(windows)]
fn robocopy(plan: &OperationPlan, workspace: &Workspace) -> Result<(), String> {
    let Some(navo_core::Step::MakeFolder { target }) = plan.steps.first() else {
        return Err("benchmark plan must start with a data set folder".to_owned());
    };
    let destination =
        super::workspace::simplified(&navo_fs::os_path(target).map_err(|error| error.message())?);
    let name = destination
        .file_name()
        .ok_or_else(|| "data set folder has no name".to_owned())?;
    let output = std::process::Command::new("robocopy")
        .arg(workspace.source.join(name))
        .arg(&destination)
        .args(["/E", "/MT:8", "/NJH", "/NJS", "/NFL", "/NDL", "/NP"])
        .output()
        .map_err(|error| format!("starting robocopy: {error}"))?;
    if output
        .status
        .code()
        .is_some_and(|code| (0..8).contains(&code))
    {
        Ok(())
    } else {
        Err(format!(
            "robocopy failed ({}): {} {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

const SAMPLE_STEPS: u64 = 10_000;

/// Per-step log that keeps the first and last 10,000 copy-step durations.
struct StepLog<'a> {
    plan: &'a OperationPlan,
    started: Instant,
    copy_index: u64,
    copy_total: u64,
    first: Duration,
    first_steps: u64,
    last: Duration,
    last_steps: u64,
}

impl<'a> StepLog<'a> {
    fn new(plan: &'a OperationPlan) -> Self {
        let copy_total = plan
            .steps
            .iter()
            .filter(|step| matches!(step, Step::CopyFile { .. }))
            .count() as u64;
        Self {
            plan,
            started: Instant::now(),
            copy_index: 0,
            copy_total,
            first: Duration::ZERO,
            first_steps: 0,
            last: Duration::ZERO,
            last_steps: 0,
        }
    }

    fn line(&self) -> Result<String, String> {
        if self.copy_index != self.copy_total
            || self.first_steps != SAMPLE_STEPS
            || self.last_steps != SAMPLE_STEPS
        {
            return Err(format!(
                "steps sample incomplete: {} of {} steps, first {}, last {}",
                self.copy_index, self.copy_total, self.first_steps, self.last_steps
            ));
        }
        let first_us = self.first.as_secs_f64() * 1_000_000.0 / self.first_steps as f64;
        let last_us = self.last.as_secs_f64() * 1_000_000.0 / self.last_steps as f64;
        let ratio = per_step_ratio(self.first, self.first_steps, self.last, self.last_steps);
        Ok(format!(
            "bench-copy: steps={} first10k={first_us:.2} last10k={last_us:.2} ratio={ratio:.2}",
            self.copy_total
        ))
    }
}

impl JobLog for StepLog<'_> {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.started);
        self.started = now;
        let Some(step) = self.plan.steps.get(seq as usize) else {
            return Ok(());
        };
        if !matches!(step, Step::CopyFile { .. }) || outcome != StepOutcome::Done {
            return Ok(());
        }
        if self.copy_index < SAMPLE_STEPS {
            self.first += elapsed;
            self.first_steps += 1;
        }
        if self.copy_index >= self.copy_total.saturating_sub(SAMPLE_STEPS) {
            self.last += elapsed;
            self.last_steps += 1;
        }
        self.copy_index += 1;
        Ok(())
    }

    fn undone(&mut self, _seq: u32, _outcome: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, _held: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

/// Returns the last per-step duration divided by the first, from each sample's totals.
pub(super) fn per_step_ratio(
    first: Duration,
    first_steps: u64,
    last: Duration,
    last_steps: u64,
) -> f64 {
    let first = first.as_secs_f64() / first_steps.max(1) as f64;
    let last = last.as_secs_f64() / last_steps.max(1) as f64;
    last / first
}
