use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use navo_core::{AppState, Command, CursorMove, Direction, Location};
use navo_fs::{FakeFs, ProviderOps};

use super::super::Browser;
use super::super::tests::settle;
use super::{
    CompareControl, CompareHooks, CompareKind, CompareRow, CompareSide, CompareSink, CompareUpdate,
    RowStatus,
};
use crate::settings::Values;

mod mode;
mod rows;
mod stopped;
mod stream;

#[derive(Default)]
pub(super) struct Calls {
    pub sink: Mutex<Option<CompareSink>>,
    pub starts: Mutex<Vec<(String, String)>>,
    pub stopped: Mutex<Vec<Option<u64>>>,
    pub dropped: AtomicBool,
    pub continued: AtomicUsize,
    /// Makes the hooks refuse every location, like a pane the app cannot read.
    pub unsupported: AtomicBool,
}

impl Calls {
    pub(super) fn send(&self, update: CompareUpdate) {
        let sink = self.sink.lock().unwrap_or_else(PoisonError::into_inner);
        (sink.as_ref().expect("a started compare"))(update);
    }

    pub(super) fn stopped(&self) -> Vec<Option<u64>> {
        self.stopped.lock().unwrap().clone()
    }
}

struct FakeControl(Arc<Calls>);

impl CompareControl for FakeControl {
    fn focus(&self, _row_ids: Vec<u64>) {}

    fn stop_row(&self, id: u64) {
        self.0.stopped.lock().unwrap().push(Some(id));
    }

    fn stop_all(&self) {
        self.0.stopped.lock().unwrap().push(None);
    }

    fn refresh(&self) {}

    fn continue_walk(&self) {
        self.0.continued.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for FakeControl {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::SeqCst);
    }
}

struct FakeHooks(Arc<Calls>);

impl CompareHooks for FakeHooks {
    fn supports(&self, _location: &Location) -> bool {
        !self.0.unsupported.load(Ordering::SeqCst)
    }

    fn start(
        &self,
        (left, right): (&Location, &Location),
        _values: &Values,
        sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        let pair = (left.as_str().to_owned(), right.as_str().to_owned());
        self.0.starts.lock().unwrap().push(pair);
        *self.0.sink.lock().unwrap() = Some(sink);
        self.0.dropped.store(false, Ordering::SeqCst);
        Box::new(FakeControl(Arc::clone(&self.0)))
    }
}

/// `left` and `right` share `docs/` and `a.txt`; `left` has `b.txt` and `old/`.
pub(super) fn sides() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for side in ["left", "right"] {
        fs.add_folder(&format!("{side}/docs"));
        fs.add_file(&format!("{side}/docs/report.txt"), 7);
        fs.add_file(&format!("{side}/a.txt"), 3);
    }
    fs.add_file("left/b.txt", 4);
    fs.add_folder("left/old");
    Arc::new(fs)
}

/// Two panes: the left one on `left`, the right one on `right` and focused.
pub(super) fn browser(fs: &Arc<FakeFs>) -> (Browser, Arc<Calls>) {
    let state = AppState::new(fs.location("left"));
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.start();
    let split = Command::LayoutSplit {
        direction: Direction::Right,
    };
    browser.command(&split).expect("split");
    let location = fs.location("right");
    browser
        .command(&Command::PaneNavigate { location })
        .expect("navigate");
    settle(&mut browser);
    let calls = Arc::new(Calls::default());
    browser.set_compare_hooks(Arc::new(FakeHooks(Arc::clone(&calls))));
    (browser, calls)
}

pub(super) fn cursor_on(browser: &mut Browser, row: u64) {
    let to = CursorMove::Row(row);
    browser
        .command(&Command::PaneCursorMove { to })
        .expect("cursor");
}

/// Locations of the left and right panes, by layout order.
pub(super) fn pane_locations(browser: &Browser) -> Vec<String> {
    let state = browser.state();
    let window = state.window(state.focused_window()).expect("window");
    window
        .layout()
        .numbered()
        .iter()
        .filter_map(|&pane| state.pane(pane))
        .map(|pane| pane.active().location().as_str().to_owned())
        .collect()
}

/// The cursor row name of the left and right panes.
pub(super) fn cursor_names(browser: &Browser) -> Vec<Option<String>> {
    let state = browser.state();
    let window = state.window(state.focused_window()).expect("window");
    window
        .layout()
        .numbered()
        .iter()
        .filter_map(|&pane| state.pane(pane))
        .map(|pane| {
            let tab = pane.active();
            let entry = tab.listing().and_then(|l| l.get(tab.view().cursor));
            entry.map(|entry| entry.name.clone())
        })
        .collect()
}

fn side(kind: CompareKind) -> Option<CompareSide> {
    Some(CompareSide {
        kind,
        size: 1,
        modified_ms: None,
    })
}

pub(super) fn row(id: u64, path: &str, status: RowStatus) -> CompareRow {
    let kind = if path.ends_with(".txt") {
        CompareKind::File
    } else {
        CompareKind::Folder
    };
    let (left, right) = match status {
        RowStatus::OnlyLeft => (side(kind), None),
        RowStatus::OnlyRight => (None, side(kind)),
        _ => (side(kind), side(kind)),
    };
    CompareRow {
        id,
        path: path.to_owned(),
        left,
        right,
        status,
        contains_differences: false,
    }
}

pub(super) fn rows(generation: u64, rows: Vec<CompareRow>) -> CompareUpdate {
    CompareUpdate::Rows {
        generation,
        rows,
        progress: Vec::new(),
    }
}
