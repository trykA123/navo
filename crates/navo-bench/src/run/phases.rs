use std::cell::RefCell;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::SystemTime;

use super::Platform;
use crate::guard::ScratchRoot;
use crate::harness::phase::{Observer, PANIC_CODE, Phase, PhaseResult, Verdict, drive};
use crate::harness::probe::ProbeError;
use crate::harness::progress::Progress;
use crate::manifest::{Manifest, Verdict as ProofVerdict};
use crate::measure::{
    IdleReport, MeasureReport, Metric, StartKind, StartReport, measure_start_and_idle,
};
use crate::privacy::{LeakScanner, Report, SafeText, SafeWriter, SafetyVerdict};
use crate::profile;
use crate::report::{PhaseOutcome, PhaseRow, Proof, RunSummary, Timings};
use crate::safe_text;
use crate::source::{ReadOnlySource, WalkConfig};
use crate::timings::{Plan, measure_folder_timings};

/// Code of the proof phase when the owner's folder changed or could not be fully checked.
pub(super) const PROOF_FAILED: u32 = 90;

const NO_FOLDER: SafeText = safe_text!("no folder chosen");
const STOPPED: SafeText = safe_text!("the run stopped unexpectedly");

pub(super) struct Inputs<'a> {
    pub(super) root: &'a ScratchRoot,
    pub(super) source: Option<&'a ReadOnlySource>,
    pub(super) exe: Result<&'a Path, ProbeError>,
    pub(super) quick: bool,
}

/// Everything the report is built from.
pub(super) struct Outcome {
    pub(super) reports: Vec<Report>,
    pub(super) timings: Timings,
    /// A phase failed or the folder proof failed.
    pub(super) failed: bool,
    pub(super) verdict: SafetyVerdict,
}

impl Outcome {
    /// What is left when the phases stopped outside every guard: no proof, no numbers.
    pub(super) fn stopped(folder: bool) -> Self {
        let proof = if folder {
            Proof::Unfinished
        } else {
            Proof::NotMeasured { reason: NO_FOLDER }
        };
        Self {
            verdict: proof.verdict(),
            reports: vec![Report::Run(RunSummary {
                proof,
                phases: Vec::new(),
            })],
            timings: Timings::NotMeasured { reason: STOPPED },
            failed: true,
        }
    }
}

/// Progress on the console plus every phase verdict, in order.
struct Recorder<W> {
    progress: Progress<W>,
    phases: Vec<PhaseRow>,
}

impl<W: Write> Observer for Recorder<W> {
    fn started(&mut self, name: SafeText) {
        self.progress.started(name);
    }

    fn finished(&mut self, name: SafeText, verdict: Verdict) {
        self.progress.finished(name, verdict);
        let outcome = match verdict {
            Verdict::Ok => PhaseOutcome::Ok,
            Verdict::Failed(code) => PhaseOutcome::Failed { code },
            Verdict::NotMeasured(reason) => PhaseOutcome::NotMeasured { reason },
        };
        self.phases.push(PhaseRow { name, outcome });
    }
}

impl<W: Write> Recorder<W> {
    /// Records a phase that could not start at all.
    fn skipped(&mut self, name: SafeText, reason: SafeText) {
        self.started(name);
        self.finished(name, Verdict::NotMeasured(reason));
    }

    /// Runs `step`; a panic outside its phases becomes the failed row `name`.
    fn guarded<T>(&mut self, name: SafeText, step: impl FnOnce(&mut Self) -> T) -> Option<T> {
        let ran = catch_unwind(AssertUnwindSafe(|| step(&mut *self)));
        if ran.is_err() {
            self.finished(name, Verdict::Failed(PANIC_CODE));
        }
        ran.ok()
    }
}

fn not_tester(error: ProbeError) -> SafeText {
    match error {
        ProbeError::Missing => safe_text!("navo.exe not found next to navo-bench"),
        ProbeError::NotTester => safe_text!("navo.exe is not a tester build"),
        ProbeError::Unreadable(_) => safe_text!("navo.exe could not be read"),
    }
}

/// Runs every phase in order: manifest before, data profile, index, machine, start and idle,
/// folder timings, manifest after with the proof. A failing or panicking phase never stops the
/// rest, and the manifest after always runs.
pub(super) fn run_all(
    inputs: &Inputs<'_>,
    platform: &mut impl Platform,
    console: &mut impl Write,
    scanner: &LeakScanner,
) -> Outcome {
    let mut recorder = Recorder {
        progress: Progress::new(SafeWriter::new(console, scanner.clone())),
        phases: Vec::new(),
    };
    let mut reports = Vec::new();
    let before = recorder
        .guarded(safe_text!("folder, index and machine phases"), |recorder| {
            first_phases(inputs, platform, recorder, &mut reports)
        })
        .flatten();
    let timings = recorder
        .guarded(safe_text!("start, idle and timing phases"), |recorder| {
            tester_phases(inputs, platform, recorder, &mut reports)
        })
        .unwrap_or(Timings::NotMeasured { reason: STOPPED });
    let proof = proof_phase(inputs.source, before.as_ref(), &mut recorder);
    let failed = proof.failed()
        || recorder
            .phases
            .iter()
            .any(|phase| matches!(phase.outcome, PhaseOutcome::Failed { .. }));
    let verdict = proof.verdict();
    reports.push(Report::Run(RunSummary {
        proof,
        phases: recorder.phases,
    }));
    Outcome {
        reports,
        timings,
        failed,
        verdict,
    }
}

/// Manifest before, data profile, index and machine; returns the manifest taken before.
fn first_phases<W: Write>(
    inputs: &Inputs<'_>,
    platform: &mut impl Platform,
    recorder: &mut Recorder<W>,
    reports: &mut Vec<Report>,
) -> Option<Manifest> {
    let before = RefCell::new(None);
    let collected = RefCell::new(Vec::new());
    let platform = RefCell::new(platform);
    let source = inputs.source;
    let phases = vec![
        Phase::new(safe_text!("folder check before"), || {
            let Some(source) = source else {
                return PhaseResult::NotMeasured(NO_FOLDER);
            };
            *before.borrow_mut() = Some(Manifest::capture(source));
            PhaseResult::Done
        }),
        Phase::new(safe_text!("data profile"), || {
            let Some(source) = source else {
                return PhaseResult::NotMeasured(NO_FOLDER);
            };
            let (profile, _) = profile::collect(source, WalkConfig::default(), SystemTime::now());
            collected.borrow_mut().extend(profile.reports());
            PhaseResult::Done
        }),
        Phase::new(safe_text!("index"), || {
            let exe = match inputs.exe {
                Ok(exe) => exe,
                Err(error) => return PhaseResult::NotMeasured(not_tester(error)),
            };
            match platform.borrow_mut().index(inputs.root, exe) {
                Ok(index) => {
                    collected.borrow_mut().push(Report::Index(index));
                    PhaseResult::Done
                }
                Err(reason) => PhaseResult::NotMeasured(reason),
            }
        }),
        Phase::new(safe_text!("machine"), || {
            let machine = platform.borrow().machine(inputs.root.path());
            collected.borrow_mut().push(Report::Machine(machine));
            PhaseResult::Done
        }),
    ];
    drive(phases, recorder);
    reports.append(&mut collected.into_inner());
    before.into_inner()
}

/// Start and idle, then the folder timings; both need the tester navo.exe.
fn tester_phases<W: Write>(
    inputs: &Inputs<'_>,
    platform: &mut impl Platform,
    recorder: &mut Recorder<W>,
    reports: &mut Vec<Report>,
) -> Timings {
    let exe = match inputs.exe {
        Ok(exe) => exe,
        Err(error) => {
            let reason = not_tester(error);
            reports.extend(unmeasured_start(reason));
            recorder.skipped(safe_text!("start and idle"), reason);
            recorder.skipped(safe_text!("folder timings"), reason);
            return Timings::NotMeasured { reason };
        }
    };
    let mut host = platform.measure_host(inputs.root, exe);
    reports.extend(measure_start_and_idle(&mut *host, recorder));
    drop(host);
    let owner = inputs.source.and_then(|source| source.share().ok());
    let plan = Plan {
        owner: owner.is_some(),
        generated: !inputs.quick,
    };
    let mut host = platform.timings_host(inputs.root, exe, owner);
    let timed = measure_folder_timings(&mut *host, plan, recorder);
    let timings = Timings::from_reports(&timed);
    reports.extend(timed);
    timings
}

fn unmeasured_start(reason: SafeText) -> [Report; 4] {
    [
        MeasureReport::Start(StartReport::unavailable(StartKind::Cold, reason)),
        MeasureReport::Start(StartReport::unavailable(StartKind::Warm, reason)),
        MeasureReport::Idle(IdleReport::unavailable(reason)),
        MeasureReport::ExeSize {
            bytes: Metric::missing(reason),
        },
    ]
    .map(Report::Measure)
}

/// Takes the manifest again and compares; anything but equal fails the run.
fn proof_phase<W: Write>(
    source: Option<&ReadOnlySource>,
    before: Option<&Manifest>,
    recorder: &mut Recorder<W>,
) -> Proof {
    let start = match source {
        Some(_) => Proof::Unfinished,
        None => Proof::NotMeasured { reason: NO_FOLDER },
    };
    let proof = RefCell::new(start);
    let phases = vec![Phase::new(safe_text!("folder check after"), || {
        let Some(source) = source else {
            return PhaseResult::NotMeasured(NO_FOLDER);
        };
        let Some(before) = before else {
            return PhaseResult::Failed(PROOF_FAILED);
        };
        let after = Manifest::capture(source);
        let diff = Manifest::diff(before, &after);
        if diff.verdict == ProofVerdict::Equal {
            let counts = after.counts();
            *proof.borrow_mut() = Proof::Equal {
                files: counts.files,
                folders: counts.dirs,
            };
            PhaseResult::Done
        } else {
            *proof.borrow_mut() = Proof::Fail { diff };
            PhaseResult::Failed(PROOF_FAILED)
        }
    })];
    drive(phases, recorder);
    proof.into_inner()
}
