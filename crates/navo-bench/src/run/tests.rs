use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, UNIX_EPOCH};

use serde_json::Value;

use super::output::default_name;
use super::{Exit, Platform, run};
use crate::fixture;
use crate::guard::test_support::{TestRecycler, append, mkdir, tree, write};
use crate::guard::{Recycler, ScratchRoot};
use crate::harness::picker::PickerError;
use crate::index::{IndexReport, ServiceState};
use crate::machine::{self, MachineSummary};
use crate::privacy::{SafeText, assert_no_leaks};
use crate::source::ReadOnlySource;
use crate::timings::{Capture, Launch, Subject, TimingError, Window};
use crate::{measure, timings};

#[path = "safety_tests.rs"]
mod safety_tests;

/// The fakes of the measure and timings tests behind one platform.
struct Fakes {
    exe_dir: tempfile::TempDir,
    recycler: TestRecycler,
    picked: Option<PathBuf>,
    fail_measure: bool,
    panic_in_measure_host: bool,
    /// A file the fake timings host appends to, as a writing benchmark would.
    write_into: Option<PathBuf>,
    timing_events: Rc<RefCell<Vec<String>>>,
    measure_hosts: Cell<u32>,
}

impl Fakes {
    fn new(tester: bool) -> Self {
        let exe_dir = fixture::tempdir().unwrap();
        fixture::stand_in_exe(exe_dir.path(), 4096, tester).unwrap();
        Self {
            exe_dir,
            recycler: TestRecycler::default(),
            picked: None,
            fail_measure: false,
            panic_in_measure_host: false,
            write_into: None,
            timing_events: Rc::default(),
            measure_hosts: Cell::new(0),
        }
    }

    fn dir(&self) -> &Path {
        self.exe_dir.path()
    }

    fn timing_events(&self) -> Vec<String> {
        self.timing_events.borrow().clone()
    }

    /// Names left in the exe folder that look like run roots.
    fn run_roots_left(&self) -> usize {
        fs::read_dir(self.dir())
            .unwrap()
            .filter(|entry| {
                let name = entry.as_ref().unwrap().file_name();
                name.to_string_lossy().starts_with("run-")
            })
            .count()
    }
}

struct WritingTimings {
    inner: timings::fake::Fake,
    write_into: Option<PathBuf>,
}

impl timings::Host for WritingTimings {
    fn prepare(&mut self) -> Result<(), TimingError> {
        if let Some(path) = &self.write_into {
            append(path, b"written during the run");
        }
        self.inner.prepare()
    }
    fn generate(&mut self, files: u32) -> Result<(), TimingError> {
        self.inner.generate(files)
    }
    fn run_bench(&mut self, subject: Subject, run: u32) -> Result<String, TimingError> {
        self.inner.run_bench(subject, run)
    }
    fn launch(&mut self, launch: Launch) -> Result<Box<dyn Window>, TimingError> {
        self.inner.launch(launch)
    }
    fn read(&mut self, capture: Capture, run: u32) -> Option<String> {
        self.inner.read(capture, run)
    }
    fn pause(&mut self, time: Duration) {
        self.inner.pause(time);
    }
    fn elapsed(&self) -> Duration {
        self.inner.elapsed()
    }
}

impl Platform for Fakes {
    fn exe_dir(&self) -> Option<PathBuf> {
        Some(self.dir().to_path_buf())
    }

    fn pick_folder(&mut self) -> Result<Option<PathBuf>, PickerError> {
        Ok(self.picked.clone())
    }

    fn recycler(&self) -> &dyn Recycler {
        &self.recycler
    }

    fn measure_host<'a>(&self, _root: &'a ScratchRoot, _exe: &Path) -> Box<dyn measure::Host + 'a> {
        self.measure_hosts.set(self.measure_hosts.get() + 1);
        assert!(!self.panic_in_measure_host, "a fault outside any phase");
        let mut fake = measure::fake::Fake::new();
        fake.fail_prepare = self.fail_measure;
        Box::new(fake)
    }

    fn timings_host<'a>(
        &self,
        _root: &'a ScratchRoot,
        _exe: &Path,
        _owner: Option<ReadOnlySource>,
    ) -> Box<dyn timings::Host + 'a> {
        let mut inner = timings::fake::Fake::new();
        inner.events = Rc::clone(&self.timing_events);
        Box::new(WritingTimings {
            inner,
            write_into: self.write_into.clone(),
        })
    }

    fn index(&mut self, _root: &ScratchRoot, _exe: &Path) -> Result<IndexReport, SafeText> {
        Ok(IndexReport::unavailable(ServiceState::Running))
    }

    fn machine(&self, root: &Path) -> MachineSummary {
        machine::summarize(root)
    }
}

struct Ran {
    exit: Exit,
    console: Vec<u8>,
    out: PathBuf,
}

impl Ran {
    fn file(&self, name: &str) -> String {
        fs::read_to_string(self.out.join(name)).unwrap()
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.file("report.json")).unwrap()
    }

    fn run_summary(&self) -> Value {
        let json = self.json();
        let reports = json["reports"].as_array().unwrap();
        let run = reports.iter().find(|report| report["kind"] == "run");
        run.unwrap().clone()
    }
}

fn bench(fakes: &mut Fakes, folder: Option<&Path>, extra: &[&str]) -> Ran {
    let out = fakes.dir().join("out");
    let mut args = vec![OsString::from("--out"), out.clone().into_os_string()];
    if let Some(folder) = folder {
        args.extend([OsString::from("--folder"), folder.into()]);
    }
    args.extend(extra.iter().map(OsString::from));
    let mut console = Vec::new();
    let exit = run(args, fakes, &mut console);
    Ran { exit, console, out }
}

fn planted<'a>(fx: &'a fixture::Fixture, fakes: &'a Fakes) -> Vec<&'a str> {
    vec![
        fixture::PRIVATE_FOLDER,
        fixture::PRIVATE_FILE,
        fixture::PRIVATE_EXTENSION,
        fixture::PRIVATE_QUERY,
        fixture::PRIVATE_CONTENT,
        fixture::MARKER,
        measure::fake::SECRET,
        timings::fake::SECRET,
        fx.root().to_str().unwrap(),
        fakes.dir().to_str().unwrap(),
    ]
}

#[test]
fn medium_full_run_writes_both_reports_leaks_nothing_and_leaves_the_folder_equal() {
    let fx = fixture::set().unwrap();
    let mut fakes = Fakes::new(true);
    let before = tree(fx.root());
    let ran = bench(&mut fakes, Some(fx.root()), &[]);
    assert_eq!(ran.exit, Exit::Done);
    assert_eq!(
        tree(fx.root()),
        before,
        "the owner's folder must not change"
    );
    let json = ran.json();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(ran.run_summary()["proof"]["state"], "equal");
    let html = ran.file("report.html");
    for section in [
        "summary",
        "safety",
        "targets",
        "start-idle",
        "index",
        "timings",
        "profile",
        "machine",
        "baselines",
    ] {
        assert!(
            html.contains(&format!("<section id=\"{section}\">")),
            "{section}"
        );
    }
    let mut everything = ran.console.clone();
    everything.extend(ran.file("report.json").bytes());
    everything.extend(html.bytes());
    assert_no_leaks(&everything, &planted(&fx, &fakes)).unwrap();
    assert_eq!(fakes.run_roots_left(), 0, "the run root goes to the bin");
    assert_eq!(fakes.recycler.calls.get(), 1);
}

#[test]
fn medium_a_failing_phase_exits_1_still_writes_the_report_and_cleans_up() {
    let fx = fixture::set().unwrap();
    let mut fakes = Fakes::new(true);
    fakes.fail_measure = true;
    let ran = bench(&mut fakes, Some(fx.root()), &["--quick"]);
    assert_eq!(ran.exit, Exit::Failed);
    let phases = ran.run_summary()["phases"].clone();
    let failed = phases
        .as_array()
        .unwrap()
        .iter()
        .filter(|phase| phase["outcome"]["outcome"] == "failed")
        .count();
    assert_eq!(failed, 1);
    assert!(ran.file("report.html").contains("failed, code"));
    assert_eq!(fakes.run_roots_left(), 0);
}

#[test]
fn medium_quick_skips_only_the_generated_folder() {
    let fx = fixture::set().unwrap();
    let mut quick = Fakes::new(true);
    assert_eq!(
        bench(&mut quick, Some(fx.root()), &["--quick"]).exit,
        Exit::Done
    );
    let events = quick.timing_events();
    assert!(!events.iter().any(|event| event.starts_with("generate")));
    assert!(events.iter().any(|event| event.starts_with("bench owner")));
    assert!(
        !events
            .iter()
            .any(|event| event.starts_with("bench generated"))
    );
    let mut full = Fakes::new(true);
    assert_eq!(bench(&mut full, Some(fx.root()), &[]).exit, Exit::Done);
    assert!(full.timing_events().contains(&"generate 100000".to_owned()));
}

#[test]
fn medium_a_plain_build_marks_its_phases_not_measured() {
    let fx = fixture::set().unwrap();
    let mut fakes = Fakes::new(false);
    let ran = bench(&mut fakes, Some(fx.root()), &["--quick"]);
    assert_eq!(ran.exit, Exit::Done);
    assert_eq!(fakes.measure_hosts.get(), 0);
    assert!(fakes.timing_events().is_empty());
    let json = ran.json();
    assert_eq!(json["timings"]["state"], "not-measured");
    assert!(ran.file("report.html").contains("not a tester build"));
    assert_eq!(ran.run_summary()["proof"]["state"], "equal");
}

#[test]
fn medium_cancelled_picker_runs_without_a_folder_into_a_default_output_folder() {
    let mut fakes = Fakes::new(true);
    let mut console = Vec::new();
    let exit = run(vec![OsString::from("--quick")], &mut fakes, &mut console);
    assert_eq!(exit, Exit::Done);
    let outputs: Vec<PathBuf> = fs::read_dir(fakes.dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("navo-bench-")
        })
        .collect();
    assert_eq!(outputs.len(), 1);
    let json = fs::read_to_string(outputs[0].join("report.json")).unwrap();
    let json: Value = serde_json::from_str(&json).unwrap();
    let reports = json["reports"].as_array().unwrap();
    let run = reports
        .iter()
        .find(|report| report["kind"] == "run")
        .unwrap();
    assert_eq!(run["proof"]["state"], "not-measured");
    assert!(String::from_utf8_lossy(&console).contains("no folder chosen"));
}

#[test]
fn small_bad_arguments_exit_2_and_create_nothing() {
    let mut fakes = Fakes::new(true);
    let before = tree(fakes.dir());
    let mut console = Vec::new();
    let exit = run(
        vec![OsString::from(fixture::PRIVATE_FOLDER)],
        &mut fakes,
        &mut console,
    );
    assert_eq!(exit, Exit::Usage);
    assert!(String::from_utf8_lossy(&console).contains("usage"));
    assert_no_leaks(&console, &[fixture::PRIVATE_FOLDER]).unwrap();
    assert_eq!(tree(fakes.dir()), before);
}

#[test]
fn medium_an_existing_output_folder_is_refused_and_left_alone() {
    let fx = fixture::set().unwrap();
    let mut fakes = Fakes::new(true);
    mkdir(&fakes.dir().join("out"));
    write(&fakes.dir().join("out/report.json"), b"older report");
    let ran = bench(&mut fakes, Some(fx.root()), &[]);
    assert_eq!(ran.exit, Exit::Usage);
    assert_eq!(ran.file("report.json"), "older report");
    assert_eq!(fakes.run_roots_left(), 0);
    assert!(String::from_utf8_lossy(&ran.console).contains("already exists"));
}

#[test]
fn medium_an_output_folder_inside_the_owner_folder_is_refused() {
    let fx = fixture::set().unwrap();
    let mut fakes = Fakes::new(true);
    let before = tree(fx.root());
    let args = vec![
        OsString::from("--folder"),
        fx.root().into(),
        OsString::from("--out"),
        fx.root().join("report").into(),
    ];
    let mut console = Vec::new();
    assert_eq!(run(args, &mut fakes, &mut console), Exit::Usage);
    assert_eq!(tree(fx.root()), before);
    assert_eq!(fakes.run_roots_left(), 0);
    assert_no_leaks(&console, &planted(&fx, &fakes)).unwrap();
}

#[test]
fn medium_a_refused_owner_folder_exits_2_without_an_output_folder() {
    let mut fakes = Fakes::new(true);
    let exe_dir = fakes.dir().to_path_buf();
    let args = vec![
        OsString::from("--folder"),
        std::env::current_exe().unwrap().parent().unwrap().into(),
        OsString::from("--out"),
        exe_dir.join("out").into(),
    ];
    let mut console = Vec::new();
    assert_eq!(run(args, &mut fakes, &mut console), Exit::Usage);
    assert!(!exe_dir.join("out").exists());
    assert_eq!(fakes.run_roots_left(), 0);
    assert!(String::from_utf8_lossy(&console).contains("exe-folder"));
}

#[test]
fn small_default_output_name_is_the_utc_time() {
    let at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    assert_eq!(default_name(at), "navo-bench-20231114-221320");
    let leap = UNIX_EPOCH + Duration::from_secs(951_782_400);
    assert_eq!(default_name(leap), "navo-bench-20000229-000000");
}
