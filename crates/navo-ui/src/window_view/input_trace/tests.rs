use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{FocusHandle, KeyBinding, Render, TestAppContext};

use super::*;
use crate::prelude::*;
use crate::window_view::tests::fake;
use navo_fs::{Provider, ProviderOps};

gpui_kit::actions!(input_trace_tests, [ReadMetadata]);

struct Probe {
    focus: FocusHandle,
    provider: Arc<dyn ProviderOps>,
    location: navo_core::Location,
    paint: Rc<Cell<bool>>,
}

struct Rig {
    window: gpui_kit::AnyWindowHandle,
    paint: Rc<Cell<bool>>,
}

impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let trace = frame(window, cx);
        let paint = self.paint.clone();
        div()
            .size_full()
            .track_focus(&self.focus)
            .children(trace)
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, window, cx| {
                window.dispatch_action(Box::new(ReadMetadata), cx);
            })
            .on_action(cx.listener(|view, _: &ReadMetadata, _, cx| {
                view.provider.metadata(&view.location).unwrap();
                view.provider.metadata(&view.location).unwrap();
                cx.background_executor()
                    .advance_clock(Duration::from_millis(5));
                cx.notify();
            }))
            .child(gpui_kit::canvas(
                |_, _, _| (),
                move |_, (), _, cx| {
                    if paint.replace(false) {
                        cx.background_executor()
                            .advance_clock(Duration::from_millis(3));
                    }
                },
            ))
    }
}

fn open_probe(cx: &mut TestAppContext, trace: Option<&InputTrace>) -> Rig {
    cx.update(InputTrace::hooks(trace));
    cx.update(|cx| cx.bind_keys([KeyBinding::new("ctrl-j", ReadMetadata, None)]));
    let fs = fake();
    let location = fs.location("home/a.txt");
    let provider = trace.map_or_else(
        || fs.clone() as Arc<dyn ProviderOps>,
        |trace| trace.provider(fs.clone()),
    );
    let paint = Rc::<Cell<bool>>::default();
    let render_clock = paint.clone();
    let window = cx
        .add_window(|window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            Probe {
                focus,
                provider,
                location,
                paint: render_clock,
            }
        })
        .into();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    Rig { window, paint }
}

fn read(path: &std::path::Path) -> Vec<Record> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn trace_line_round_trips_as_json() {
    let record = Record {
        action: "key:ctrl-j".into(),
        key_to_frame_ms: 12.5,
        ui_busy_ms: 7.25,
        disk_calls: 2,
    };
    let mut bytes = Vec::new();
    write_record(&mut bytes, &record).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(serde_json::from_slice::<Record>(&bytes).unwrap(), record);
}

#[gpui_kit::test]
fn one_action_counts_disk_calls_and_includes_dispatch_and_render(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("keys.jsonl");
    let trace = InputTrace::new(path.clone()).unwrap();
    let rig = open_probe(cx, Some(&trace));
    cx.update_window(rig.window, |_, window, cx| {
        window.dispatch_keystroke(gpui_kit::Keystroke::parse("ctrl-j").unwrap(), cx)
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_millis(11));
    rig.paint.set(true);
    cx.update_window(rig.window, |_, window, cx| {
        window.simulate_next_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    drop(trace);
    let records = read(&path);
    assert_eq!(records.len(), 1);
    assert!(records[0].action.ends_with("::ReadMetadata"));
    assert_eq!(records[0].disk_calls, 2);
    assert_eq!(records[0].key_to_frame_ms, 16.);
    assert_eq!(records[0].ui_busy_ms, 8.);
}

#[gpui_kit::test]
fn tracing_is_none_without_the_flag(cx: &mut TestAppContext) {
    let rig = open_probe(cx, None);
    cx.update_window(rig.window, |_, window, cx| {
        window.dispatch_keystroke(gpui_kit::Keystroke::parse("ctrl-j").unwrap(), cx)
    })
    .unwrap();
    cx.update(|cx| assert!(cx.try_global::<State>().is_none()));
}

#[gpui_kit::test]
fn raw_keys_and_mouse_inputs_each_produce_a_line(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("keys.jsonl");
    let previous = r#"{"action":"previous","key_to_frame_ms":1.0,"ui_busy_ms":0.5,"disk_calls":0}"#;
    std::fs::write(&path, format!("{previous}\n")).unwrap();
    let trace = InputTrace::new(path.clone()).unwrap();
    let rig = open_probe(cx, Some(&trace));
    cx.update(|cx| {
        cx.intercept_keystrokes(|event, _, cx| {
            if event.keystroke.key == "x" {
                cx.stop_propagation();
            }
        })
        .detach();
    });
    cx.update_window(rig.window, |_, window, cx| {
        window.dispatch_keystroke(gpui_kit::Keystroke::parse("x").unwrap(), cx);
    })
    .unwrap();
    cx.update_window(rig.window, |_, window, cx| {
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent::default()),
            cx,
        );
    })
    .unwrap();
    rig.paint.set(true);
    cx.update_window(rig.window, |_, window, cx| {
        window.simulate_next_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    drop(trace);
    let records = read(&path);
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].action, "previous");
    assert_eq!(records[1].action, "key:X");
    assert_eq!(records[2].action, "mouse:down");
    assert_eq!(records[1].disk_calls, 0);
    assert_eq!(records[2].disk_calls, 2);
    assert_eq!(records[2].ui_busy_ms, 8.);
}

#[gpui_kit::test]
fn an_input_that_paints_no_frame_still_gets_a_line_without_render_time(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("keys.jsonl");
    let trace = InputTrace::new(path.clone()).unwrap();
    let rig = open_probe(cx, Some(&trace));
    cx.update_window(rig.window, |_, window, cx| {
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent::default()),
            cx,
        );
        window.simulate_next_frame(cx);
        window.simulate_next_frame(cx);
    })
    .unwrap();
    drop(trace);
    let records = read(&path);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action, "mouse:move");
    assert_eq!(records[0].ui_busy_ms, 0.);
}

struct PauseSpy {
    fs: Arc<navo_fs::FakeFs>,
    pauses: AtomicUsize,
}

impl Provider for PauseSpy {
    fn id(&self) -> navo_core::ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> navo_fs::Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &navo_core::Location,
        sink: &mut dyn navo_fs::ListSink,
        cancel: &navo_fs::CancelToken,
    ) -> Result<(), navo_core::FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(
        &self,
        location: &navo_core::Location,
    ) -> Result<navo_fs::FsEntry, navo_core::FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for PauseSpy {
    fn pause_git(
        &self,
        location: &navo_core::Location,
    ) -> Result<Option<navo_fs::GitPauseGuard>, navo_core::FsError> {
        self.pauses.fetch_add(1, Ordering::SeqCst);
        self.fs.pause_git(location)
    }

    fn create_folder(&self, location: &navo_core::Location) -> Result<(), navo_core::FsError> {
        self.fs.create_folder(location)
    }

    fn create_file(&self, location: &navo_core::Location) -> Result<(), navo_core::FsError> {
        self.fs.create_file(location)
    }

    fn copy_file(
        &self,
        source: &navo_core::Location,
        target: &navo_core::Location,
        progress: &mut dyn FnMut(u64),
        cancel: &navo_fs::CancelToken,
    ) -> Result<u64, navo_core::FsError> {
        self.fs.copy_file(source, target, progress, cancel)
    }

    fn rename(
        &self,
        source: &navo_core::Location,
        target: &navo_core::Location,
    ) -> Result<(), navo_core::FsError> {
        self.fs.rename(source, target)
    }

    fn replace(
        &self,
        source: &navo_core::Location,
        target: &navo_core::Location,
    ) -> Result<(), navo_core::FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &navo_core::Location) -> Result<(), navo_core::FsError> {
        self.fs.remove_file(location)
    }

    fn remove_link(&self, location: &navo_core::Location) -> Result<(), navo_core::FsError> {
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(
        &self,
        location: &navo_core::Location,
    ) -> Result<(), navo_core::FsError> {
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &navo_core::Location) -> Result<(), navo_core::FsError> {
        self.fs.recycle(location)
    }
}

#[test]
fn pause_git_reaches_the_inner_provider_through_the_trace() {
    let dir = tempfile::tempdir().unwrap();
    let trace = InputTrace::new(dir.path().join("pause.jsonl")).unwrap();
    let spy = Arc::new(PauseSpy {
        fs: fake(),
        pauses: AtomicUsize::new(0),
    });
    let inner: Arc<dyn ProviderOps> = spy.clone();
    let wrapped = trace.provider(inner);
    let location = spy.fs.location("home/a");
    assert!(wrapped.pause_git(&location).unwrap().is_none());
    assert_eq!(spy.pauses.load(Ordering::SeqCst), 1);
}

#[test]
fn content_hash_reaches_the_inner_provider_and_counts_one_call() {
    let fs = fake();
    fs.set_hash("home/a.txt", [7; 32]);
    let location = fs.location("home/a.txt");
    let calls = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let wrapped = provider::CountingFs::new(fs, calls.clone());

    let hash = wrapped.content_hash(&location, &navo_fs::CancelToken::new());

    assert_eq!(hash.unwrap(), [7; 32]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

struct Quiet;

impl navo_fs::JobLog for Quiet {
    fn step(&mut self, _: u32, _: navo_fs::StepOutcome) -> Result<(), navo_core::FsError> {
        Ok(())
    }

    fn undone(&mut self, _: u32, _: navo_fs::StepOutcome) -> Result<(), navo_core::FsError> {
        Ok(())
    }

    fn journal_held(&mut self, _: &navo_fs::HeldFile) -> Result<bool, navo_core::FsError> {
        Ok(false)
    }
}

#[test]
fn an_overwrite_on_a_volume_without_ids_resolves_paths_through_the_inner_provider() {
    let fs = fake();
    fs.add_file("dest/a.txt", 9);
    fs.set_without_ids("home");
    fs.set_without_ids("dest");
    let calls = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let wrapped = provider::CountingFs::new(fs.clone(), calls);
    let cancel = navo_fs::CancelToken::new();
    let (source, target) = (fs.location("home/a.txt"), fs.location("dest"));
    let items = navo_fs::scan(&wrapped, &[source], &cancel).unwrap();
    let existing = navo_fs::existing_at(&wrapped, &items, &target, &cancel).unwrap();
    let plan = navo_core::plan(&navo_core::PlanRequest {
        kind: navo_core::OpKind::Copy,
        items,
        target: Some(target),
        existing,
        policy: navo_core::ConflictPolicy::OverwriteAll,
        same_volume: false,
        protected: Vec::new(),
    });
    let mut ctx = navo_fs::JobContext {
        ops: &wrapped,
        log: &mut Quiet,
        cancel: &cancel,
        progress: &mut |_| {},
    };

    let outcome = navo_fs::run_job(&plan, &[], &mut ctx);

    assert_eq!(outcome, navo_fs::JobOutcome::Done);
    assert_eq!(fs.metadata(&fs.location("dest/a.txt")).unwrap().size, 3);
}

#[test]
fn a_move_copies_and_removes_its_source_through_the_inner_provider() {
    let fs = fake();
    let (source, target) = (fs.location("home/a.txt"), fs.location("dest/a.txt"));
    let calls = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let wrapped = provider::CountingFs::new(fs.clone(), calls.clone());
    let cancel = navo_fs::CancelToken::new();
    let request = navo_fs::MoveCopy {
        source: &source,
        target: &target,
        created: None,
    };
    let control = navo_fs::CopyControl {
        progress: &mut |_| {},
        cancel: &cancel,
    };

    let (written, read) = wrapped.copy_moved_file(request, control).unwrap();
    assert_eq!((written, calls.load(Ordering::SeqCst)), (3, 1));
    wrapped
        .remove_copied_source(&source, read.unwrap())
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(fs.metadata(&source).is_err());
}
