use std::ffi::OsString;
use std::path::PathBuf;

use super::args::{Args, ArgsError, parse};
use super::phase::{Observer, Phase, PhaseResult, Verdict, drive};
use super::picker::PickerError;
use super::probe::{ProbeError, probe_in};
use super::progress::Progress;
use super::start::{StartError, open_with};
use super::stats::{iqr, median, p95, summarize};
use crate::fixture;
use crate::privacy::{ErrorCode, LeakScanner, PathRole, SafeText, SafeWriter, assert_no_leaks};
use crate::safe_text;

fn words(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

#[test]
fn no_flag_means_the_picker() {
    assert_eq!(parse(words(&[])).unwrap(), Args::default());
}

#[test]
fn all_flags_parse() {
    let args = parse(words(&["--quick", "--folder", "a", "--out", "b"])).unwrap();
    assert!(args.quick);
    assert_eq!(args.folder, Some(PathBuf::from("a")));
    assert_eq!(args.out, Some(PathBuf::from("b")));
}

#[test]
fn bad_arguments_are_refused_without_echoing_them() {
    let secret = fixture::PRIVATE_FOLDER;
    assert_eq!(parse(words(&[secret])), Err(ArgsError::UnknownArgument));
    assert_eq!(parse(words(&["--folder"])), Err(ArgsError::MissingValue));
    assert_eq!(
        parse(words(&["--out", "--quick"])),
        Err(ArgsError::MissingValue)
    );
    assert_eq!(
        parse(words(&["--quick", "--quick"])),
        Err(ArgsError::Repeated)
    );
    assert_eq!(
        parse(words(&["--folder", "a", "--folder", "b"])),
        Err(ArgsError::Repeated)
    );
    let shown = ArgsError::UnknownArgument.to_string();
    assert_no_leaks(shown.as_bytes(), &[secret]).unwrap();
}

#[test]
fn median_handles_odd_and_even_counts() {
    assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
    assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[1.0, f64::NAN]), None);
}

#[test]
fn p95_uses_nearest_rank_and_labels_small_sets_as_max() {
    let twenty: Vec<f64> = (1..=20).map(f64::from).collect();
    let summary = summarize(&twenty).unwrap();
    assert_eq!(summary.p95, 19.0);
    assert!(!summary.p95_is_max);
    let nineteen: Vec<f64> = (1..=19).map(f64::from).collect();
    let summary = summarize(&nineteen).unwrap();
    assert_eq!(summary.p95, 19.0);
    assert!(summary.p95_is_max);
    assert_eq!(p95(&[7.0]), Some(7.0));
}

#[test]
fn iqr_flags_noise_above_fifteen_percent_of_the_median() {
    let calm = [100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0];
    assert!(!summarize(&calm).unwrap().noisy);
    let noisy = [60.0, 70.0, 100.0, 100.0, 100.0, 100.0, 140.0, 150.0];
    let summary = summarize(&noisy).unwrap();
    assert_eq!(iqr(&noisy), Some(summary.iqr));
    assert!(summary.noisy);
    assert_eq!(summarize(&[5.0]).unwrap().iqr, 0.0);
}

#[derive(Default)]
struct Recorder(Vec<&'static str>);

impl Observer for Recorder {
    fn started(&mut self, _name: SafeText) {
        self.0.push("start");
    }
    fn finished(&mut self, _name: SafeText, _verdict: Verdict) {
        self.0.push("end");
    }
}

#[test]
fn a_failed_or_panicking_phase_does_not_stop_the_rest() {
    let mut ran = Vec::new();
    let phases = vec![
        Phase::new(safe_text!("first"), || {
            ran.push(1);
            PhaseResult::Failed(7)
        }),
        Phase::new(safe_text!("second"), || -> PhaseResult { panic!("boom") }),
        Phase::new(safe_text!("third"), || {
            PhaseResult::NotMeasured(safe_text!("no window"))
        }),
        Phase::new(safe_text!("fourth"), || PhaseResult::Done),
    ];
    let mut recorder = Recorder::default();
    let verdicts = drive(phases, &mut recorder);
    assert_eq!(ran, [1]);
    assert_eq!(recorder.0.len(), 8);
    assert!(matches!(verdicts[0].1, Verdict::Failed(7)));
    assert!(matches!(
        verdicts[1].1,
        Verdict::Failed(super::phase::PANIC_CODE)
    ));
    assert!(matches!(verdicts[2].1, Verdict::NotMeasured(_)));
    assert!(matches!(verdicts[3].1, Verdict::Ok));
}

#[test]
fn progress_prints_names_and_verdicts_without_paths() {
    let scanner = LeakScanner::new(&[fixture::PRIVATE_FOLDER]).unwrap();
    let mut sink = Vec::new();
    {
        let mut progress = Progress::new(SafeWriter::new(&mut sink, scanner));
        let phases = vec![
            Phase::new(safe_text!("start"), || PhaseResult::Done),
            Phase::new(safe_text!("idle"), || PhaseResult::Failed(3)),
            Phase::new(safe_text!("index"), || {
                PhaseResult::NotMeasured(safe_text!("service missing"))
            }),
        ];
        drive(phases, &mut progress);
    }
    let text = String::from_utf8(sink).unwrap();
    assert!(text.contains("start") && text.contains("failed") && text.contains("service missing"));
    assert_no_leaks(text.as_bytes(), &[fixture::PRIVATE_FOLDER]).unwrap();
}

#[test]
fn probe_accepts_a_tester_build_even_across_a_chunk_boundary() {
    for padding in [10, (1 << 20) - 5, (1 << 20) + 3] {
        let dir = tempfile::tempdir().unwrap();
        fixture::stand_in_exe(dir.path(), padding, true).unwrap();
        assert!(probe_in(dir.path()).is_ok(), "padding {padding}");
    }
}

#[test]
fn probe_refuses_a_plain_build_and_a_missing_exe() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(probe_in(dir.path()), Err(ProbeError::Missing));
    fixture::stand_in_exe(dir.path(), 4096, false).unwrap();
    assert_eq!(probe_in(dir.path()), Err(ProbeError::NotTester));
}

#[test]
fn folder_argument_goes_straight_to_the_read_only_source() {
    let fx = fixture::set().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let args = Args {
        folder: Some(fx.root().to_path_buf()),
        ..Args::default()
    };
    let opened = open_with(&args, scratch.path(), || panic!("picker must not run"));
    assert!(opened.is_ok());
}

#[test]
fn picker_result_is_opened_and_cancel_or_missing_picker_is_reported() {
    let fx = fixture::set().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let args = Args::default();
    let root = fx.root().to_path_buf();
    assert!(open_with(&args, scratch.path(), || Ok(Some(root))).is_ok());
    let cancelled = open_with(&args, scratch.path(), || Ok(None));
    assert_eq!(cancelled.unwrap_err(), StartError::Cancelled);
    let none = open_with(&args, scratch.path(), || Err(PickerError::Unsupported));
    assert_eq!(none.unwrap_err(), StartError::PathRequired);
}

#[test]
fn refusal_prints_role_and_code_only() {
    let fx = fixture::set().unwrap();
    let exe = std::env::current_exe().unwrap();
    let args = Args {
        folder: exe.parent().map(PathBuf::from),
        ..Args::default()
    };
    let error = open_with(&args, fx.root(), || panic!("no picker")).unwrap_err();
    let StartError::Refused(refused) = error else {
        panic!("expected a refusal");
    };
    assert_eq!(refused.role, PathRole::OwnerFolder);
    assert_eq!(refused.code, ErrorCode::Refused("exe-folder"));
    let shown = error.to_string();
    assert_no_leaks(
        shown.as_bytes(),
        &[fixture::PRIVATE_FOLDER, fixture::MARKER],
    )
    .unwrap();
    let exe_text = exe.parent().unwrap().to_string_lossy().into_owned();
    assert!(!shown.contains(&exe_text));
}
