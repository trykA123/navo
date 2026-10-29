use std::cell::Cell;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{FocusHandle, KeyBinding, Render, TestAppContext};

use super::*;
use crate::prelude::*;
use crate::window_view::tests::fake;

gpui_kit::actions!(input_trace_tests, [ReadMetadata]);

struct Probe {
    focus: FocusHandle,
    provider: Arc<dyn ProviderOps>,
    location: rusty_core::Location,
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
