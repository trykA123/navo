use std::{path::Path, time::Instant};

use rusty_core::{ConflictPolicy, FsError, Location, OpKind, OperationPlan, PlanRequest, plan};
use rusty_fs::{
    AutoTuning, CancelToken, JobContext, JobLog, JobOutcome, LocalFs, StepOutcome, WorkerCounts,
    copy_workers, drive_class, existing_at, run_job, run_job_parallel, scan,
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
        ops: LocalFs::with_auto_tuning(tuning),
        workers,
    };
    let methods = [
        Method::Standard,
        Method::Auto,
        #[cfg(windows)]
        Method::Robocopy,
    ];
    let mut rows = Vec::new();
    for &set in &options.sets {
        let source = workspace.source.join(set.name);
        eprintln!(
            "bench-copy: generating {} ({} bytes)",
            set.name,
            set.bytes()
        );
        fixture::generate(&source, set)?;
        let plan = build_plan(&benchmark.ops, &source, &workspace.target)?;
        for method in methods {
            let seconds = benchmark.measure_method(&plan, method, options.runs)?;
            rows.push(Measurement {
                set,
                method: method.name(),
                seconds,
            });
        }
    }
    Ok(report::markdown(&rows, classes, workers, options.runs))
}

impl Benchmark<'_> {
    fn measure_method(
        &self,
        plan: &OperationPlan,
        method: Method,
        runs: usize,
    ) -> Result<f64, String> {
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
        report::median(&mut samples).ok_or_else(|| "no benchmark samples".to_owned())
    }

    fn run_copy(&self, plan: &OperationPlan, method: Method) -> Result<(), String> {
        let cancel = CancelToken::new();
        let mut log = BenchLog;
        let mut progress = |_: &rusty_fs::Progress| {};
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
}

fn location(path: &Path) -> Result<Location, String> {
    #[cfg(unix)]
    let provider_path = {
        use std::os::unix::ffi::OsStrExt;
        rusty_core::ProviderPath::from_bytes(path.as_os_str().as_bytes())
    };
    #[cfg(windows)]
    let provider_path = {
        use std::os::windows::ffi::OsStrExt;
        rusty_core::ProviderPath::from_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
    };
    Location::local(provider_path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(windows)]
fn robocopy(plan: &OperationPlan, workspace: &Workspace) -> Result<(), String> {
    let Some(rusty_core::Step::MakeFolder { target }) = plan.steps.first() else {
        return Err("benchmark plan must start with a data set folder".to_owned());
    };
    let destination =
        super::workspace::simplified(&rusty_fs::os_path(target).map_err(|error| error.message())?);
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
