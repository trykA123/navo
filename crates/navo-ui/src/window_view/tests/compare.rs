use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext, VisualTestContext};
use navo_core::Location;

use super::{NavoWindow, StartOptions, fake, open_options, options_for, press, settle, two_panes};
use crate::browser::{
    CompareControl, CompareFilter, CompareHooks, CompareKind, CompareRow, CompareSide, CompareSink,
    CompareUpdate, RowStatus,
};
use crate::commands::Keymap;
use crate::compare_list::BarEvent;
use crate::settings::Values;
use crate::window_view::OpenHooks;

const KEYMAP: &str = include_str!("../../commands/keymap.toml");

type Sink = Arc<Mutex<Option<CompareSink>>>;

struct Idle(Arc<AtomicUsize>);

impl CompareControl for Idle {
    fn focus(&self, _row_ids: Vec<u64>) {}
    fn stop_row(&self, _id: u64) {}
    fn stop_all(&self) {}
    fn refresh(&self) {}
    fn continue_walk(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct Hooks(Sink, Arc<AtomicUsize>);

impl CompareHooks for Hooks {
    fn supports(&self, _location: &Location) -> bool {
        true
    }

    fn start(
        &self,
        _sides: (&Location, &Location),
        _values: &Values,
        sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        *self.0.lock().unwrap() = Some(sink);
        Box::new(Idle(Arc::clone(&self.1)))
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    sink: Sink,
    continued: Arc<AtomicUsize>,
    docs: String,
}

/// Both panes on `home`; the keymap is parsed again so the compare flag of this thread applies.
fn rig(cx: &mut TestAppContext) -> Rig {
    rig_with(cx, Keymap::parse(KEYMAP).expect("keymap"))
}

fn rig_with(cx: &mut TestAppContext, keymap: Keymap) -> Rig {
    let fs = fake();
    let sink = Sink::default();
    let continued = Arc::new(AtomicUsize::new(0));
    let options = StartOptions {
        open: OpenHooks {
            compare: Some(Arc::new(Hooks(sink.clone(), Arc::clone(&continued)))),
            ..OpenHooks::none()
        },
        ..options_for(fs.clone(), two_panes(&fs))
    };
    let (window, view) = open_options(cx, options);
    cx.update(|cx| crate::window_view::keys::rebind_app_keys(keymap, cx));
    let docs = fs.location("home/docs").as_str().to_owned();
    Rig {
        window,
        view,
        sink,
        continued,
        docs,
    }
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn comparing(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.browser.compare_mode().is_some())
    }

    fn folders(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            view.panes()
                .iter()
                .filter_map(|&pane| state.pane(pane))
                .map(|pane| pane.active().location().as_str().to_owned())
                .collect()
        })
    }

    fn send_docs(&self, cx: &mut TestAppContext) {
        let side = Some(CompareSide {
            kind: CompareKind::Folder,
            size: 0,
            modified_ms: None,
        });
        let docs = CompareRow {
            id: 1,
            path: "docs".into(),
            left: side,
            right: side,
            status: RowStatus::Same,
            contains_differences: false,
        };
        let sink = self
            .sink
            .lock()
            .unwrap()
            .clone()
            .expect("a started compare");
        sink(CompareUpdate::Rows {
            generation: 1,
            rows: vec![docs],
            progress: Vec::new(),
        });
        settle(cx, &self.view);
        self.view.update(cx, |view, _| {
            view.browser
                .compare_mode_mut()
                .expect("compare mode")
                .select("docs");
        });
    }
}

#[gpui_kit::test]
fn ctrl_shift_d_does_nothing_while_compare_is_hidden(cx: &mut TestAppContext) {
    let _hidden = crate::compare_visible::hide_compare();
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    assert!(!rig.comparing(cx));
    assert!(rig.sink.lock().unwrap().is_none());
}

#[gpui_kit::test]
fn enter_opens_the_folder_in_both_panes_and_esc_brings_them_back(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    let before = rig.folders(cx);
    rig.press(cx, "ctrl-shift-d");
    assert!(rig.comparing(cx));
    rig.send_docs(cx);
    rig.press(cx, "enter");
    assert_eq!(rig.folders(cx), [rig.docs.clone(), rig.docs.clone()]);
    rig.press(cx, "escape");
    assert!(!rig.comparing(cx));
    assert_eq!(rig.folders(cx), before);
}

impl Rig {
    fn send_files(&self, cx: &mut TestAppContext, names: &[&str]) {
        let side = Some(CompareSide {
            kind: CompareKind::File,
            size: 3,
            modified_ms: None,
        });
        let rows = (1u64..)
            .zip(names)
            .map(|(id, name)| CompareRow {
                id,
                path: (*name).to_owned(),
                left: side,
                right: side,
                status: RowStatus::Different,
                contains_differences: false,
            })
            .collect();
        let sink = self
            .sink
            .lock()
            .unwrap()
            .clone()
            .expect("a started compare");
        sink(CompareUpdate::Rows {
            generation: 1,
            rows,
            progress: Vec::new(),
        });
        settle(cx, &self.view);
    }

    fn bar(&self, cx: &mut TestAppContext, event: BarEvent) {
        let view = self.view.clone();
        let mut visual = VisualTestContext::from_window(self.window, cx);
        view.update_in(&mut visual, |view, window, cx| {
            view.bar_event(&event, window, cx);
        });
    }

    fn selected(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            let mode = view.browser.compare_mode()?;
            mode.selected_path().map(str::to_owned)
        })
    }
}

#[gpui_kit::test]
fn the_cursor_keys_walk_the_compare_rows(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_files(cx, &["a.txt", "b.txt", "c.txt"]);
    assert_eq!(rig.selected(cx), None);
    rig.press(cx, "down");
    assert_eq!(rig.selected(cx).as_deref(), Some("a.txt"));
    rig.press(cx, "down");
    rig.press(cx, "down");
    rig.press(cx, "down");
    assert_eq!(rig.selected(cx).as_deref(), Some("c.txt"));
    rig.press(cx, "up");
    assert_eq!(rig.selected(cx).as_deref(), Some("b.txt"));
    rig.press(cx, "home");
    assert_eq!(rig.selected(cx).as_deref(), Some("a.txt"));
}

/// A user binds Enter to another file-list command; the user keys register after the built-in ones.
#[gpui_kit::test]
fn enter_in_compare_beats_a_file_list_enter_the_user_binds(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let user = "[[bind]]\ncommand = \"files.view\"\nkeys = [\"enter\"]\n";
    let overlaid = Keymap::parse(KEYMAP)
        .expect("keymap")
        .overlay(user)
        .expect("user keymap");
    assert!(overlaid.keymap.is_user("files.view", "FileList"));
    assert!(overlaid.keymap.has_key("files.view", "enter"));
    assert!(!overlaid.keymap.has_key("files.open", "enter"));
    let rig = rig_with(cx, overlaid.keymap);
    let before = rig.folders(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_docs(cx);
    rig.press(cx, "enter");
    assert_eq!(rig.folders(cx), [rig.docs.clone(), rig.docs.clone()]);
    assert_ne!(rig.folders(cx), before);
}

#[gpui_kit::test]
fn tab_still_switches_panes_while_comparing(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    let focused = |cx: &mut TestAppContext| {
        rig.view
            .read_with(cx, |view, _| view.browser.state().focused_pane())
    };
    let before = focused(cx);
    rig.press(cx, "tab");
    assert_ne!(focused(cx), before);
}

#[gpui_kit::test]
fn the_filter_and_leave_buttons_of_the_bar_work(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.send_files(cx, &["a.txt"]);
    rig.bar(cx, BarEvent::Filter(CompareFilter::Same));
    let filter = rig.view.read_with(cx, |view, _| {
        view.browser.compare_mode().map(|m| m.filter())
    });
    assert_eq!(filter, Some(CompareFilter::Same));
    rig.bar(cx, BarEvent::Leave);
    assert!(!rig.comparing(cx));
}

#[gpui_kit::test]
fn the_stop_notice_continues_by_button_and_by_key(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "ctrl-shift-d");
    rig.press(cx, "ctrl-f7");
    assert_eq!(rig.continued.load(Ordering::SeqCst), 0);
    let sink = rig.sink.lock().unwrap().clone().expect("a started compare");
    sink(CompareUpdate::Stopped {
        generation: 1,
        entries: 200_000,
        bound: crate::browser::StopBound::Entries,
    });
    settle(cx, &rig.view);
    rig.bar(cx, BarEvent::Continue);
    assert_eq!(rig.continued.load(Ordering::SeqCst), 1);
    rig.press(cx, "ctrl-f7");
    assert_eq!(rig.continued.load(Ordering::SeqCst), 2);
    sink(CompareUpdate::Continued { generation: 1 });
    settle(cx, &rig.view);
    rig.press(cx, "ctrl-f7");
    assert_eq!(rig.continued.load(Ordering::SeqCst), 2);
}
