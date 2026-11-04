use super::*;
use crate::fixture;
use crate::index::{IndexMemory, IndexReport, MemoryPartKind, PartMemory, QueryTime, ServiceState};
use crate::machine::{DefenderState, DiskKind, FreeSpace, MachineSummary};
use crate::measure::{
    Bytes, CpuWindow, IdleReport, MeasureReport, Metric, StartKind, StartReport, Stat, Wakeups,
};
use crate::privacy::LeakScanner;
use crate::safe_text;
use crate::source::{WalkConfig, test_open};

pub(super) const MS: u64 = 1000;

pub(super) fn stat(median_ms: u64, p95_ms: u64) -> Metric<Stat> {
    Metric::Measured {
        value: Stat {
            samples: 20,
            median_micros: median_ms * MS,
            p95_micros: p95_ms * MS,
            p95_is_max: false,
            iqr_micros: 5 * MS,
            noisy: false,
        },
    }
}

pub(super) fn start(kind: StartKind, usable_ms: u64) -> Report {
    Report::Measure(MeasureReport::Start(StartReport {
        kind,
        attempted: 20,
        failed: 0,
        first_window: stat(60, 70),
        first_frame: stat(120, 140),
        first_usable: stat(usable_ms, usable_ms + 30),
    }))
}

pub(super) fn idle(private_bytes: u64, wakeups: f64) -> Report {
    let bytes = Bytes {
        private_bytes,
        working_set_bytes: private_bytes + 1_000_000,
    };
    Report::Measure(MeasureReport::Idle(IdleReport {
        memory_at_settle: Metric::Measured { value: bytes },
        cpu: Metric::Measured {
            value: CpuWindow {
                window_micros: 60_000_000,
                cpu_micros: 90_000,
            },
        },
        wakeups: Metric::Measured {
            value: Wakeups {
                total_per_second: wakeups + 60.0,
                busiest_thread_per_second: 60.0,
                without_busiest_per_second: wakeups,
            },
        },
        memory_at_end: Metric::missing(safe_text!("window was closed")),
    }))
}

pub(super) fn index(entries: u64, bytes: u64, query_ms: u64) -> Report {
    Report::Index(IndexReport {
        service: ServiceState::Running,
        cold_start_micros: Some(900 * MS),
        warm_start_micros: Some(300 * MS),
        first_answer_micros: Some(40 * MS),
        first_building_answer_micros: None,
        entries,
        building: false,
        queries: (0..3)
            .map(|id| QueryTime {
                id,
                runs: 5,
                min_micros: MS,
                median_micros: query_ms * MS,
                max_micros: 60 * MS,
                results: 12,
            })
            .collect(),
        memory: IndexMemory {
            busy: false,
            parts: vec![PartMemory {
                part: MemoryPartKind::Process,
                bytes,
                items: 1,
            }],
            volumes: Vec::new(),
        },
    })
}

pub(super) fn machine() -> Report {
    Report::Machine(MachineSummary {
        cpu_cores: 8,
        ram_gib: Some(16),
        disk: DiskKind::Ssd,
        free_space: FreeSpace::From50To200Gib,
        windows_build: Some(26100),
        defender: DefenderState::On,
    })
}

pub(super) fn timing(
    folder: TimingFolder,
    step: TimingStep,
    median_ms: u64,
    p95_ms: u64,
) -> TimingRow {
    TimingRow {
        folder,
        step,
        index: 0,
        samples: 5,
        median_micros: median_ms * MS,
        p95_micros: p95_ms * MS,
    }
}

pub(super) fn good_timings() -> Timings {
    Timings::Measured {
        rows: vec![
            timing(TimingFolder::Generated100k, TimingStep::List, 150, 160),
            timing(TimingFolder::Chosen, TimingStep::KeyToFrame, 8, 12),
            timing(TimingFolder::Chosen, TimingStep::Sort, 30, 35),
        ],
    }
}

pub(super) fn passing_reports() -> Vec<Report> {
    vec![
        start(StartKind::Cold, 380),
        start(StartKind::Warm, 200),
        idle(70_000_000, 0.5),
        Report::Measure(MeasureReport::ExeSize {
            bytes: Metric::Measured { value: 100 << 20 },
        }),
        index(1_200_000, 90_000_000, 20),
        machine(),
    ]
}

pub(super) fn row_of(report: &RunReport, id: TargetId) -> TargetRow {
    *report.targets().iter().find(|t| t.id == id).unwrap()
}

pub(super) fn profile_reports() -> Vec<Report> {
    let fx = fixture::set().unwrap();
    let source = test_open(fx.root()).unwrap();
    let (profile, _) =
        crate::profile::collect(&source, WalkConfig::default(), std::time::SystemTime::now());
    profile.reports()
}

#[test]
fn a_complete_passing_run_passes_everything_it_can_measure() {
    let report = RunReport::new(passing_reports(), good_timings());
    let tally = report.tally();
    assert_eq!((tally.fail, tally.overall), (0, Overall::Incomplete));
    for id in [
        TargetId::ColdUsable,
        TargetId::WarmFirstFrame,
        TargetId::Open100k,
        TargetId::CtrlK,
        TargetId::ExeSize,
    ] {
        assert_eq!(row_of(&report, id).verdict, Verdict::Pass, "{id:?}");
    }
    assert!(matches!(
        row_of(&report, TargetId::TraySummon).verdict,
        Verdict::NotMeasured { .. }
    ));
}

#[test]
fn limits_are_inclusive_except_for_below_targets() {
    let mut reports = passing_reports();
    reports[0] = start(StartKind::Cold, 400);
    reports[2] = idle(80_000_000, 1.0);
    let report = RunReport::new(reports, good_timings());
    assert_eq!(row_of(&report, TargetId::ColdUsable).verdict, Verdict::Pass);
    assert_eq!(
        row_of(&report, TargetId::ColdUsableSpeedPhase).verdict,
        Verdict::Pass
    );
    assert_eq!(
        row_of(&report, TargetId::IdleWakeupsFocused).verdict,
        Verdict::Pass
    );
    assert_eq!(
        row_of(&report, TargetId::IdlePrivateBytes).verdict,
        Verdict::Fail
    );
    assert_eq!(report.tally().overall, Overall::SomeFail);
}

#[test]
fn a_missed_target_fails_and_keeps_its_value() {
    let mut reports = passing_reports();
    reports[1] = start(StartKind::Warm, 300);
    let report = RunReport::new(reports, good_timings());
    let row = row_of(&report, TargetId::WarmFirstUsable);
    assert_eq!((row.verdict, row.value), (Verdict::Fail, Some(300 * MS)));
}

#[test]
fn nothing_measured_gives_reasons_not_verdicts() {
    let report = RunReport::new(
        Vec::new(),
        Timings::NotMeasured {
            reason: safe_text!("no folder chosen"),
        },
    );
    assert_eq!(report.tally().pass + report.tally().fail, 0);
    let open = row_of(&report, TargetId::Open100k);
    let Verdict::NotMeasured { reason } = open.verdict else {
        panic!("expected not measured");
    };
    assert_eq!(reason.as_str(), "no folder chosen");
    assert!(report.targets().iter().all(|t| t.value.is_none()));
}

#[test]
fn index_memory_is_compared_only_for_a_million_files_unless_already_over() {
    let small = RunReport::new(vec![index(10_000, 50_000_000, 20)], good_timings());
    assert!(matches!(
        row_of(&small, TargetId::IndexMemory).verdict,
        Verdict::NotMeasured { .. }
    ));
    let over = RunReport::new(vec![index(10_000, 100_000_000, 20)], good_timings());
    assert_eq!(row_of(&over, TargetId::IndexMemory).verdict, Verdict::Fail);
    let full = RunReport::new(vec![index(1_000_000, 99_000_000, 20)], good_timings());
    assert_eq!(row_of(&full, TargetId::IndexMemory).verdict, Verdict::Pass);
}

#[test]
fn ctrl_k_uses_the_slowest_query_median() {
    let slow = RunReport::new(vec![index(1, 1, 51)], good_timings());
    assert_eq!(row_of(&slow, TargetId::CtrlK).verdict, Verdict::Fail);
}

#[test]
fn timing_slot_feeds_open_100k_and_key_to_frame_p95() {
    let mut rows = good_timings().rows().to_vec();
    rows.push(timing(
        TimingFolder::Generated100k,
        TimingStep::KeyToFrame,
        10,
        17,
    ));
    let report = RunReport::new(Vec::new(), Timings::Measured { rows });
    assert_eq!(
        row_of(&report, TargetId::KeyToFrameP95).verdict,
        Verdict::Fail
    );
    assert_eq!(row_of(&report, TargetId::Open100k).value, Some(150 * MS));
}

#[test]
fn json_is_schema_one_with_numbers_and_enums() {
    let report = RunReport::new(passing_reports(), good_timings());
    let mut out = Vec::new();
    let mut writer = SafeWriter::new(&mut out, LeakScanner::new(&[]).unwrap());
    report.write_json(&mut writer).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(json["kind"], "document");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["targets"].as_array().unwrap().len(), TARGETS.len());
    assert_eq!(json["targets"][0]["id"], "cold-usable");
    assert_eq!(json["targets"][0]["verdict"], "pass");
    assert_eq!(json["tally"]["overall"], "incomplete");
    assert_eq!(json["timings"]["state"], "measured");
}
