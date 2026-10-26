use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{Action, AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::{Command, Direction};
use rusty_diag::perf::{PerfDocument, perf_json};
use rusty_diag::{Identity, Redactor, Salt};

use super::{fake, open_options, options_for, press, settle, two_panes};
use crate::activity::ActivityId;
use crate::activity_strip::key_of;
use crate::commands::{GLOBAL, by_name, entries};
use crate::perf::{self, Latency, PerfService, ProcessSample, Recording, SystemFacts};
use crate::window_view::FerriteWindow;
use crate::window_view::keys::{RecordPerformance, StopRecording};

const SECRET_FOLDER: &str = "home/canary-plans-7731";
const SECRET_QUERY: &str = "canary budget 7731";
const PERF: ActivityId = ActivityId::Background(1);

type Saved = Arc<Mutex<Option<(Recording, SystemFacts)>>>;

struct FakeService {
    saved: Saved,
    failures: AtomicUsize,
}

impl PerfService for FakeService {
    fn start(&self) {}

    fn sample(&self) -> ProcessSample {
        ProcessSample {
            cpu_percent: Some(3.0),
            working_set_bytes: Some(1 << 20),
            ..ProcessSample::default()
        }
    }

    fn system(&self) -> SystemFacts {
        SystemFacts {
            os: "Test OS".to_owned(),
            ..SystemFacts::default()
        }
    }

    fn save(
        &self,
        recording: &Recording,
        _: Instant,
        system: &SystemFacts,
    ) -> Result<PathBuf, String> {
        let failing = self
            .failures
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            });
        if failing.is_ok() {
            return Err("the disk is full".to_owned());
        }
        *self.saved.lock().unwrap() = Some((recording.clone(), system.clone()));
        Ok(PathBuf::from("navo-performance-test.zip"))
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    saved: Saved,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    rig_failing(cx, 0)
}

fn rig_failing(cx: &mut TestAppContext, failures: usize) -> Rig {
    let fs = fake();
    fs.add_folder(SECRET_FOLDER);
    fs.add_file("home/canary-plans-7731/canary-salary-7731.xlsx", 9);
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    let saved = Saved::default();
    perf::install(Arc::new(FakeService {
        saved: saved.clone(),
        failures: AtomicUsize::new(failures),
    }));
    Rig {
        window,
        view,
        saved,
    }
}

impl Rig {
    fn dispatch(&self, cx: &mut TestAppContext, action: impl Action) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_action(action.boxed_clone(), cx);
        })
        .unwrap();
        cx.run_until_parked();
    }

    fn label(&self, cx: &mut TestAppContext, id: &'static str) -> Option<String> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find((id, key_of(PERF)))
                .and_then(|found| found.label().map(str::to_owned))
        })
        .unwrap()
    }

    fn has_button(&self, cx: &mut TestAppContext, id: &'static str) -> bool {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find((id, key_of(PERF))).is_some()
        })
        .unwrap()
    }

    fn second(cx: &mut TestAppContext) -> Self {
        let fs = fake();
        let (window, view) = open_options(cx, options_for(fs.clone(), two_panes(&fs)));
        Self {
            window,
            view,
            saved: Saved::default(),
        }
    }

    fn click(&self, cx: &mut TestAppContext, id: &'static str) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.click((id, key_of(PERF)), cx);
        })
        .unwrap();
        cx.run_until_parked();
    }

    fn command(&self, cx: &mut TestAppContext, command: &Command) {
        self.view.update(cx, |view, cx| {
            let _ = view.browser.command(command);
            cx.notify();
        });
        settle(cx, &self.view);
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }

    fn wait(&self, cx: &mut TestAppContext, seconds: u64) {
        for _ in 0..seconds {
            cx.executor().advance_clock(Duration::from_secs(1));
            cx.run_until_parked();
        }
    }

    fn saved(&self) -> Option<(Recording, SystemFacts)> {
        self.saved.lock().unwrap().clone()
    }
}

#[test]
fn the_record_and_stop_commands_are_global_palette_commands() {
    for (name, action) in [
        ("perf.record", "ferrite_window::RecordPerformance"),
        ("perf.stop", "ferrite_window::StopRecording"),
    ] {
        let spec = by_name(name).unwrap();
        assert_eq!((spec.action, spec.context), (Some(action), GLOBAL));
        assert!(
            entries()
                .iter()
                .any(|entry| entry.spec.name == name && entry.in_palette)
        );
    }
}

#[gpui_kit::test]
fn recording_shows_a_quiet_strip_item_with_stop(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.dispatch(cx, RecordPerformance);
    assert_eq!(
        rig.label(cx, "activity-title").as_deref(),
        Some("Recording performance · 0:00")
    );
    assert!(rig.has_button(cx, "activity-stop"));
    rig.wait(cx, 83);
    assert_eq!(
        rig.label(cx, "activity-title").as_deref(),
        Some("Recording performance · 1:23")
    );
    rig.dispatch(cx, StopRecording);
}

#[gpui_kit::test]
fn recording_stops_by_itself_after_two_minutes(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.dispatch(cx, RecordPerformance);
    rig.wait(cx, 119);
    assert!(perf::elapsed(Instant::now()).is_some() && rig.saved().is_none());
    rig.wait(cx, 1);
    let (recording, system) = rig.saved().unwrap();
    assert!(perf::elapsed(Instant::now()).is_none());
    assert_eq!(system.os, "Test OS");
    assert!((118..=120).contains(&recording.process.len()));
    assert_eq!(
        rig.label(cx, "activity-title").as_deref(),
        Some("Performance recording saved")
    );
    assert!(rig.has_button(cx, "activity-open-folder"));
    rig.wait(cx, 30);
    assert!(rig.has_button(cx, "activity-dismiss"));
}

#[gpui_kit::test]
fn stop_recording_saves_at_once_and_a_second_stop_does_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.dispatch(cx, RecordPerformance);
    rig.wait(cx, 3);
    rig.dispatch(cx, StopRecording);
    let (recording, _) = rig.saved().unwrap();
    assert_eq!(recording.process.len(), 3);
    *rig.saved.lock().unwrap() = None;
    rig.dispatch(cx, StopRecording);
    assert!(rig.saved().is_none());
}

#[gpui_kit::test]
fn perf_json_holds_no_paths_names_or_search_text(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.dispatch(cx, RecordPerformance);
    let location = fake().location(SECRET_FOLDER);
    rig.view.update(cx, |view, cx| {
        let _ = view.browser.command(&Command::PaneNavigate { location });
        cx.notify();
    });
    settle(cx, &rig.view);
    press(cx, rig.window, &rig.view, "ctrl-k");
    cx.update_window(rig.window, |_, window, cx| window.input(SECRET_QUERY, cx))
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(200));
    settle(cx, &rig.view);
    press(cx, rig.window, &rig.view, "escape");
    rig.wait(cx, 2);
    rig.dispatch(cx, StopRecording);
    let (recording, system) = rig.saved().unwrap();
    assert!(!recording.frames.is_empty());
    let kinds: Vec<Latency> = recording.latencies.iter().map(|s| s.kind).collect();
    assert!(kinds.contains(&Latency::CommandCenterKey), "{kinds:?}");
    assert!(kinds.contains(&Latency::CommandCenterOpen), "{kinds:?}");
    let redactor = Redactor::new(Salt::random(), Identity::default());
    let json = perf_json(&PerfDocument::new(
        &recording,
        Instant::now(),
        &system,
        &redactor,
    ))
    .unwrap();
    let text = String::from_utf8(json).unwrap().to_lowercase();
    for needle in ["canary", "7731", "budget", "salary", "plans", "home/"] {
        assert!(!text.contains(needle), "{needle} leaked into perf.json");
    }
}

#[gpui_kit::test]
fn a_stop_from_another_window_clears_the_strip_of_the_starting_window(cx: &mut TestAppContext) {
    let first = rig(cx);
    let second = Rig::second(cx);
    first.dispatch(cx, RecordPerformance);
    first.wait(cx, 2);
    second.dispatch(cx, StopRecording);
    assert!(first.saved().is_some());
    assert_eq!(
        second.label(cx, "activity-title").as_deref(),
        Some("Performance recording saved")
    );
    first.wait(cx, 2);
    assert_eq!(first.label(cx, "activity-title"), None);
}

#[gpui_kit::test]
fn an_old_ticker_never_samples_into_a_newer_recording(cx: &mut TestAppContext) {
    let first = rig(cx);
    let second = Rig::second(cx);
    first.dispatch(cx, RecordPerformance);
    first.wait(cx, 1);
    second.dispatch(cx, StopRecording);
    second.dispatch(cx, RecordPerformance);
    second.wait(cx, 3);
    assert!(perf::elapsed(Instant::now()).is_some());
    second.dispatch(cx, StopRecording);
    let (recording, _) = first.saved().unwrap();
    assert_eq!(recording.process.len(), 3);
    assert_eq!(first.label(cx, "activity-title"), None);
}

#[gpui_kit::test]
fn closing_the_window_stops_and_saves_the_recording(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.dispatch(cx, RecordPerformance);
    rig.wait(cx, 4);
    let closing = cx
        .update_window(rig.window, |_, window, cx| {
            rig.view.update(cx, |view, cx| view.close(window, cx))
        })
        .unwrap();
    assert!(closing);
    assert!(perf::elapsed(Instant::now()).is_none());
    let (recording, _) = rig.saved().unwrap();
    assert_eq!(recording.process.len(), 4);
}

#[gpui_kit::test]
fn a_failed_save_offers_retry_and_keeps_the_recording(cx: &mut TestAppContext) {
    let rig = rig_failing(cx, 2);
    rig.dispatch(cx, RecordPerformance);
    rig.wait(cx, 2);
    rig.dispatch(cx, StopRecording);
    assert!(rig.saved().is_none());
    assert_eq!(
        rig.label(cx, "activity-title").as_deref(),
        Some("Performance recording not saved")
    );
    assert!(rig.has_button(cx, "activity-retry") && rig.has_button(cx, "activity-dismiss"));
    rig.dispatch(cx, RecordPerformance);
    assert!(perf::elapsed(Instant::now()).is_none());
    rig.click(cx, "activity-retry");
    assert!(rig.saved().is_none() && rig.has_button(cx, "activity-retry"));
    rig.click(cx, "activity-retry");
    let (recording, _) = rig.saved().unwrap();
    assert_eq!(recording.process.len(), 2);
    assert_eq!(
        rig.label(cx, "activity-title").as_deref(),
        Some("Performance recording saved")
    );
}

#[gpui_kit::test]
fn dismissing_a_failed_save_drops_the_recording(cx: &mut TestAppContext) {
    let rig = rig_failing(cx, 1);
    rig.dispatch(cx, RecordPerformance);
    rig.wait(cx, 1);
    rig.dispatch(cx, StopRecording);
    rig.click(cx, "activity-dismiss");
    rig.wait(cx, 1);
    assert_eq!(rig.label(cx, "activity-title"), None);
    rig.dispatch(cx, RecordPerformance);
    assert!(perf::elapsed(Instant::now()).is_some());
    rig.dispatch(cx, StopRecording);
    assert_eq!(rig.saved().unwrap().0.process.len(), 0);
}

#[gpui_kit::test]
fn only_real_navigation_counts_as_a_folder_open(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.dispatch(cx, RecordPerformance);
    let docs = fake().location("home/docs");
    let dest = fake().location("dest");
    rig.command(
        cx,
        &Command::PaneOpenTab {
            location: Some(docs),
        },
    );
    rig.command(cx, &Command::PaneActivateTab { tab: 0 });
    rig.command(
        cx,
        &Command::LayoutFocus {
            direction: Direction::Right,
        },
    );
    rig.command(cx, &Command::PaneNavigate { location: dest });
    rig.dispatch(cx, StopRecording);
    let (recording, _) = rig.saved().unwrap();
    let opens = recording
        .latencies
        .iter()
        .filter(|sample| sample.kind == Latency::FolderOpen)
        .count();
    assert_eq!(opens, 1);
}
