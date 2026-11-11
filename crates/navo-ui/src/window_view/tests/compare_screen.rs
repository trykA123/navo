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
use crate::compare_diff::{ByteProgress, ByteRegionView, DiffUpdate, LargeHunk, PairKind};
use crate::settings::Values;
use crate::window_view::OpenHooks;
use crate::window_view::compare_screen::FileState;

pub(super) mod body;
mod refresh;
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
    rig_with_files(cx, width, super::super::FileHooks::none())
}

/// The same rig, opening pair sides through `files`.
pub(super) fn rig_with_files(
    cx: &mut TestAppContext,
    width: gpui_kit::Pixels,
    files: super::super::FileHooks,
) -> Rig {
    let fs = fake();
    let sink = Sink::default();
    let diff = Arc::new(FakeDiffHooks::default());
    let options = StartOptions {
        files,
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
    pub(super) fn dispatch(&self, cx: &mut TestAppContext, action: &dyn gpui_kit::Action) {
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

    fn large_hunks(&self, cx: &mut TestAppContext) -> Option<Vec<LargeHunk>> {
        self.view.read_with(cx, |view, _| {
            view.file_large_hunks().map(<[LargeHunk]>::to_vec)
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

    pub(super) fn push(
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
fn a_large_snapshot_acknowledges_only_after_the_view_applies_it(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    let (first, second) = (large_hunk(0..2), large_hunk(5..6));
    let snapshot = |hunks: Vec<LargeHunk>, ack| DiffUpdate::Large {
        request: 1,
        hunks,
        steps: Vec::new(),
        progress: None,
        ack,
    };
    rig.push(cx, &sink, snapshot(vec![first.clone(), second.clone()], 1));
    assert_eq!(rig.diff.acks(), vec![1]);
    assert_eq!(
        rig.large_hunks(cx),
        Some(vec![first, second.clone()]),
        "the acknowledged snapshot's hunks are applied"
    );
    rig.push(cx, &sink, snapshot(vec![second.clone()], 2));
    assert_eq!(rig.diff.acks(), vec![1, 2]);
    assert_eq!(
        rig.large_hunks(cx),
        Some(vec![second]),
        "an evicted hunk is gone before its release is acknowledged"
    );
}

/// A large hunk over `rows` on both sides.
fn large_hunk(rows: std::ops::Range<u64>) -> LargeHunk {
    LargeHunk {
        left_rows: rows.clone(),
        right_rows: rows.clone(),
        left_bytes: rows.clone(),
        right_bytes: rows,
    }
}

#[gpui_kit::test]
fn newer_large_snapshots_coalesce_and_keep_only_the_last_ack(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    for ack in [3, 7] {
        sink(DiffUpdate::Large {
            request: 1,
            hunks: Vec::new(),
            steps: Vec::new(),
            progress: None,
            ack,
        });
    }
    settle(cx, &rig.view);
    assert_eq!(rig.diff.acks(), vec![7], "one snapshot, the newest ack");
}

#[test]
fn byte_deltas_merge_into_one_pending_snapshot() {
    let mut inbox = Vec::new();
    let first = DiffUpdate::Bytes {
        request: 1,
        align: crate::compare_diff::HexAlign::Offset,
        run: 1,
        regions: vec![ByteRegionView {
            left: 0..1,
            right: 0..1,
            changed: 1,
        }],
        progress: Some(ByteProgress {
            done: 1,
            total: 2,
            coarse: false,
        }),
    };
    let second = DiffUpdate::Bytes {
        request: 1,
        align: crate::compare_diff::HexAlign::Offset,
        run: 1,
        regions: vec![ByteRegionView {
            left: 1..2,
            right: 1..2,
            changed: 1,
        }],
        progress: Some(ByteProgress {
            done: 2,
            total: 2,
            coarse: false,
        }),
    };
    crate::window_view::compare_screen::coalesce(&mut inbox, 1, first);
    crate::window_view::compare_screen::coalesce(&mut inbox, 1, second);
    assert_eq!(inbox.len(), 1);
    let DiffUpdate::Bytes {
        regions, progress, ..
    } = &inbox[0].1
    else {
        panic!("a byte snapshot");
    };
    assert_eq!(regions.len(), 2, "deltas are kept");
    assert_eq!(progress.map(|p| p.done), Some(2));
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
fn compare_screen_drops_updates_for_an_old_request(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    let opened = |request| DiffUpdate::Opened {
        request,
        kind: PairKind::Image,
        text: None,
    };
    rig.push(cx, &sink, opened(1));
    rig.push(cx, &sink, opened(2));
    let stale = DiffUpdate::Failed {
        request: 1,
        text: "old".into(),
    };
    rig.push(cx, &sink, stale);
    assert!(
        matches!(rig.state(cx), Some(FileState::Opened { .. })),
        "an update for the old request changes nothing"
    );
    let current = DiffUpdate::Failed {
        request: 2,
        text: "current".into(),
    };
    rig.push(cx, &sink, current);
    assert_eq!(rig.state(cx), Some(FileState::Failed("current".into())));
}

#[gpui_kit::test]
fn compare_screen_drops_a_stale_opened_update(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    let opened = |request, kind| DiffUpdate::Opened {
        request,
        kind,
        text: None,
    };
    rig.push(cx, &sink, opened(2, PairKind::Binary));
    rig.push(
        cx,
        &sink,
        opened(1, PairKind::Unavailable("compare-reason-unsupported")),
    );
    assert_eq!(
        rig.state(cx),
        Some(FileState::Opened {
            kind: PairKind::Binary,
            text: None
        }),
        "a stale Opened must not roll the request id back"
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
