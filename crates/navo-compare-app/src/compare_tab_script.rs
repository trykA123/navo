//! Test helper: a scripted compare engine for the compare-tab tests. It keeps the sink and can
//! deliver fixed rows on the calling thread, so no foreign thread wakes the GPUI test scheduler.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use sidewise_core::Location;
use sidewise_ui::browser::{
    CompareControl, CompareHooks, CompareKind, CompareRow, CompareSide, CompareSink, CompareTotals,
    CompareUpdate, RowStatus,
};
use sidewise_ui::compare_diff::{
    CompareOptions, DiffHooks, DiffSink, PairControl, PairRequest, StreamControl, StreamSink,
    ViewedKey,
};

use crate::screens::compare_host::TEST_DIFF_HOOKS;
use crate::screens::compare_tab::install_test_hooks;
use crate::shell::Shell;
use crate::test_window::open;

/// The sink the fake engine was given; a test can send updates through it.
pub(crate) type SharedSink = Arc<Mutex<Option<CompareSink>>>;

/// A compare engine that keeps the sink and sends its script (if any) while `start` runs.
pub(crate) struct Held {
    sink: SharedSink,
    script: Vec<CompareUpdate>,
}

impl Held {
    /// An engine that sends nothing by itself.
    pub(crate) fn new(sink: SharedSink) -> Self {
        Self {
            sink,
            script: Vec::new(),
        }
    }

    /// An engine that sends `script` on the thread that calls `start`.
    pub(crate) fn scripted(sink: SharedSink, script: Vec<CompareUpdate>) -> Self {
        Self { sink, script }
    }
}

impl CompareHooks for Held {
    /// Every location can be compared in this fixture.
    fn supports(&self, _location: &Location) -> bool {
        true
    }

    /// Keeps the sink, sends the script on this thread and returns an idle control.
    fn start(
        &self,
        _sides: (&Location, &Location),
        _options: &CompareOptions,
        sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        for update in &self.script {
            sink(update.clone());
        }
        *self.sink.lock().expect("sink lock") = Some(sink);
        Box::new(Idle)
    }
}

/// A compare control that ignores every request.
struct Idle;

impl CompareControl for Idle {
    /// Ignores a focus request.
    fn focus(&self, _row_ids: Vec<u64>) {}
    /// Ignores a stop request for one row.
    fn stop_row(&self, _id: u64) {}
    /// Ignores a stop request for the whole walk.
    fn stop_all(&self) {}
    /// Ignores a refresh request.
    fn refresh(&self) {}
    /// Ignores a request to resume a stopped walk.
    fn continue_walk(&self) {}
}

/// Diff hooks that open pairs and streams without any worker thread and send no update.
struct QuietDiff;

/// A pair or stream control that ignores every request.
struct QuietControl;

impl PairControl for QuietControl {
    /// Ignores a span request.
    fn spans(&self, _rows: Range<u32>) {}
    /// Ignores a reload.
    fn refresh(&self, _request: u64) {}
}

impl StreamControl for QuietControl {
    /// Ignores a new item list.
    fn set_items(&self, _generation: u64, _items: Vec<(PairRequest, bool)>) {}
    /// Ignores a focus request.
    fn focus(&self, _ids: Vec<u64>) {}
    /// Ignores a Viewed mark.
    fn set_viewed(&self, _id: u64, _key: ViewedKey, _viewed: bool) {}
    /// Ignores a search.
    fn search(&self, _query: u64, _text: String) {}
    /// Ignores a reload.
    fn reload(&self, _id: u64) {}
}

impl DiffHooks for QuietDiff {
    /// Opens a pair that never answers.
    fn open_pair(
        &self,
        _request: PairRequest,
        _options: &CompareOptions,
        _sink: DiffSink,
    ) -> Box<dyn PairControl> {
        Box::new(QuietControl)
    }

    /// Opens a stream that never answers.
    fn open_stream(&self, _options: &CompareOptions, _sink: StreamSink) -> Box<dyn StreamControl> {
        Box::new(QuietControl)
    }
}

/// One file or folder side of a scripted row.
fn side(kind: CompareKind) -> Option<CompareSide> {
    Some(CompareSide {
        kind,
        size: 3,
        modified_ms: None,
    })
}

/// The fixed walk: differing `a.txt` and `b.txt`, folder `sub` with a difference, same `c.txt`.
pub(crate) fn rows_script() -> Vec<CompareUpdate> {
    let file = |id, path, status| {
        CompareRow::new(
            id,
            path,
            status,
            (side(CompareKind::File), side(CompareKind::File)),
        )
    };
    let mut folder = CompareRow::new(
        3,
        "sub",
        RowStatus::Different,
        (side(CompareKind::Folder), side(CompareKind::Folder)),
    );
    folder.contains_differences = true;
    folder.differences_inside = 1;
    let rows = vec![
        file(0, "a.txt", RowStatus::Different),
        file(1, "b.txt", RowStatus::Different),
        file(2, "c.txt", RowStatus::Same),
        folder,
    ];
    let totals = CompareTotals {
        rows: 4,
        ..CompareTotals::default()
    };
    vec![
        CompareUpdate::Rows {
            generation: 0,
            rows,
            progress: Vec::new(),
        },
        CompareUpdate::Totals {
            generation: 0,
            totals,
        },
        CompareUpdate::Finished { generation: 0 },
    ]
}

/// Installs the scripted compare engine (sending `script` on the test thread) and the quiet diff
/// hooks, so no foreign thread touches the test scheduler.
pub(crate) fn install_scripted(cx: &mut TestAppContext, script: Vec<CompareUpdate>) {
    TEST_DIFF_HOOKS.with_borrow_mut(|slot| *slot = Some(Arc::new(QuietDiff)));
    let hooks = Arc::new(Held::scripted(Arc::default(), script));
    cx.update(|cx| install_test_hooks(hooks, cx));
}

/// Opens a shell whose compare engine delivers `script` on the test thread and whose screens
/// open pairs on quiet diff hooks.
pub(crate) fn scripted_shell_with(
    cx: &mut TestAppContext,
    script: Vec<CompareUpdate>,
) -> (AnyWindowHandle, Entity<Shell>) {
    let (window, view) = open(cx, Shell::new);
    install_scripted(cx, script);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    (window, view)
}

/// Opens a shell scripted with the finished `rows_script` walk.
pub(crate) fn scripted_shell(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Shell>) {
    scripted_shell_with(cx, rows_script())
}
