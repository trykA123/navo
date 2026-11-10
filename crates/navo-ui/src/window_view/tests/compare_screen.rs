//! The compare screen host (V14): Enter on a file row opens the file diff through the fake
//! `DiffHooks`, Esc returns to the same row, and late updates for a closed screen are dropped.

use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_core::Location;

use super::{
    NavoWindow, StartOptions, fake, open_options_sized, options_for, press, settle, two_panes,
};
use crate::browser::{
    CompareControl, CompareHooks, CompareKind, CompareRow, CompareSide, CompareSink, CompareUpdate,
    RowStatus,
};
use crate::commands::Keymap;
use crate::compare_diff::fake::FakeDiffHooks;
use crate::compare_diff::{DiffUpdate, PairKind};
use crate::settings::Values;
use crate::window_view::OpenHooks;
use crate::window_view::compare_screen::FileState;

mod body;
mod view;

const KEYMAP: &str = include_str!("../../commands/keymap.toml");

type Sink = Arc<Mutex<Option<CompareSink>>>;

/// A folder compare that never walks; the test sends its rows.
struct Idle;

impl CompareControl for Idle {
    fn focus(&self, _row_ids: Vec<u64>) {}
    fn stop_row(&self, _id: u64) {}
    fn stop_all(&self) {}
    fn refresh(&self) {}
    fn continue_walk(&self) {}
}

struct Hooks(Sink);

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
        Box::new(Idle)
    }
}

pub(super) struct Rig {
    pub(super) window: AnyWindowHandle,
    pub(super) view: Entity<NavoWindow>,
    pub(super) diff: Arc<FakeDiffHooks>,
    /// The fake file system both panes show.
    pub(super) fs: Arc<navo_fs::FakeFs>,
    /// The sink of the started folder compare; a test sends more rows through it.
    pub(super) compare: CompareSink,
}

/// Both panes on `home`, compared, with rows `a.txt` (id 1) and `b.txt` (id 2); `a.txt` is selected.
pub(super) fn rig(cx: &mut TestAppContext) -> Rig {
    rig_sized(cx, gpui_kit::px(1000.))
}

/// The same rig in a window `width` wide.
pub(super) fn rig_sized(cx: &mut TestAppContext, width: gpui_kit::Pixels) -> Rig {
    let fs = fake();
    let sink = Sink::default();
    let diff = Arc::new(FakeDiffHooks::default());
    let options = StartOptions {
        open: OpenHooks {
            compare: Some(Arc::new(Hooks(sink.clone()))),
            diff: Some(diff.clone()),
            ..OpenHooks::none()
        },
        ..options_for(fs.clone(), two_panes(&fs))
    };
    let (window, view) = open_options_sized(cx, options, width);
    let keymap = Keymap::parse(KEYMAP).expect("keymap");
    cx.update(|cx| crate::window_view::keys::rebind_app_keys(keymap, cx));
    let mut rig = Rig {
        window,
        view,
        diff,
        fs: fs.clone(),
        compare: Arc::new(|_| {}),
    };
    rig.press(cx, "ctrl-shift-d");
    let side = Some(CompareSide {
        kind: CompareKind::File,
        size: 3,
        modified_ms: None,
    });
    let rows = (1u64..)
        .zip(["a.txt", "b.txt"])
        .map(|(id, name)| CompareRow {
            id,
            path: name.to_owned(),
            left: side,
            right: side,
            status: RowStatus::Different,
            contains_differences: false,
        })
        .collect();
    let send = sink.lock().unwrap().clone().expect("a started compare");
    rig.compare = send.clone();
    send(CompareUpdate::Rows {
        generation: 1,
        rows,
        progress: Vec::new(),
    });
    settle(cx, &rig.view);
    rig.press(cx, "down");
    rig
}

impl Rig {
    pub(super) fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    /// Runs `action` on the focused element, as a key or a menu entry would.
    fn dispatch(&self, cx: &mut TestAppContext, action: &dyn gpui_kit::Action) {
        cx.update_window(self.window, |_, window, cx| {
            window.dispatch_action(action.boxed_clone(), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    pub(super) fn state(&self, cx: &mut TestAppContext) -> Option<FileState> {
        self.view.read_with(cx, |view, _| {
            view.file_screen().map(|(_, state)| state.clone())
        })
    }

    fn selected(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            let mode = view.browser.compare_mode()?;
            mode.selected_path().map(str::to_owned)
        })
    }

    /// Whether the open file screen, or else the focused pane, holds focus.
    fn focused(&self, cx: &mut TestAppContext, on_screen: bool) -> bool {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| match on_screen {
                true => view.file_screen_focused(window),
                false => {
                    let pane = view.browser.state().focused_pane();
                    view.handle(pane, cx).is_focused(window)
                }
            })
        })
        .expect("window")
    }

    fn push(
        &self,
        cx: &mut TestAppContext,
        sink: &crate::compare_diff::DiffSink,
        update: DiffUpdate,
    ) {
        sink(update);
        settle(cx, &self.view);
    }
}

#[gpui_kit::test]
fn compare_screen_enter_on_a_file_row_opens_it_with_the_fake_hooks(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    assert_eq!(rig.selected(cx).as_deref(), Some("a.txt"));
    rig.press(cx, "enter");
    let asked = rig.diff.requests();
    assert_eq!(asked.len(), 1);
    assert_eq!((asked[0].id, asked[0].path.as_str()), (1, "a.txt"));
    assert!(asked[0].left.is_some() && asked[0].right.is_some());
    assert_eq!(rig.state(cx), Some(FileState::Loading));
    assert!(rig.focused(cx, true));
}

#[gpui_kit::test]
fn compare_screen_shows_the_opened_pair_and_a_failure_text(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    let opened = DiffUpdate::Opened {
        request: 1,
        kind: PairKind::Binary,
        text: None,
    };
    rig.push(cx, &sink, opened);
    let binary = FileState::Opened {
        kind: PairKind::Binary,
        text: None,
    };
    assert_eq!(rig.state(cx), Some(binary));
    let failed = DiffUpdate::Failed {
        request: 1,
        text: "Access denied".into(),
    };
    rig.push(cx, &sink, failed);
    assert_eq!(
        rig.state(cx),
        Some(FileState::Failed("Access denied".into()))
    );
}

#[gpui_kit::test]
fn compare_screen_esc_returns_to_the_same_row(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    assert!(rig.state(cx).is_some());
    rig.press(cx, "escape");
    assert_eq!(rig.state(cx), None);
    assert!(rig.diff.dropped(), "closing cancels the pair");
    let comparing = rig
        .view
        .read_with(cx, |view, _| view.browser.compare_mode().is_some());
    assert!(comparing, "Esc pops the screen, not the compare");
    assert_eq!(rig.selected(cx).as_deref(), Some("a.txt"));
    assert!(rig.focused(cx, false));
    rig.press(cx, "down");
    assert_eq!(rig.selected(cx).as_deref(), Some("b.txt"));
}

#[gpui_kit::test]
fn compare_screen_drops_a_late_update_for_a_closed_screen(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let old = rig.diff.pair_sink();
    rig.press(cx, "escape");
    let late = DiffUpdate::Failed {
        request: 1,
        text: "late".into(),
    };
    rig.push(cx, &old, late.clone());
    assert_eq!(rig.state(cx), None, "a late update opens nothing");
    rig.press(cx, "enter");
    rig.push(cx, &old, late);
    assert_eq!(
        rig.state(cx),
        Some(FileState::Loading),
        "the old pair's update is dropped"
    );
    let fresh = rig.diff.pair_sink();
    let opened = DiffUpdate::Opened {
        request: 1,
        kind: PairKind::Image,
        text: None,
    };
    rig.push(cx, &fresh, opened);
    assert!(matches!(rig.state(cx), Some(FileState::Opened { .. })));
}

#[gpui_kit::test]
fn compare_screen_esc_over_a_stream_returns_to_the_stream(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let stream = rig.view.update(cx, |view, cx| view.put_stream_screen(cx));
    rig.press(cx, "escape");
    assert_eq!(rig.state(cx), None);
    let has_screen = rig
        .view
        .read_with(cx, |view, _| view.compare.screen.is_some());
    assert!(has_screen, "the Stream stays");
    let on_stream = cx
        .update_window(rig.window, |_, window, _| stream.is_focused(window))
        .expect("window");
    assert!(on_stream);
}
