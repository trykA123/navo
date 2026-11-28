use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, px, size,
};
use navo_core::find::{FileHits, LineHit};
use navo_core::{EntryKind, Location, ProviderPath, SavedFind};
use navo_fs::find::{FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use super::super::find_window::test_raw_handle;
use super::super::open_requests;
use super::super::{
    FindHooks, NavoWindow, OpenRequest, OpenSender, RawWindowHandle, ResidentHooks,
};
use super::{fake, open_options, options_for, settle, two_panes};
use crate::find_results::book::FindBook;
use crate::settings::{Change, Field, Value, Values};

type OwnerCalls = Arc<Mutex<Vec<(String, String)>>>;
type PinCalls = Arc<Mutex<Vec<(String, bool)>>>;
type ShownCalls = Arc<Mutex<Vec<(Option<String>, bool)>>>;
type Saves = Rc<RefCell<Vec<usize>>>;

fn raw_tag(handle: RawWindowHandle) -> String {
    format!("{handle:?}")
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    owner_calls: OwnerCalls,
    pin_calls: PinCalls,
    shown_calls: ShownCalls,
    sender: OpenSender,
    saves: Saves,
}

struct TrayResident;

impl ResidentHooks for TrayResident {
    fn apply(&self, _values: &Values) {}

    fn key_taken(&self) -> bool {
        false
    }

    fn clipboard_key_taken(&self) -> bool {
        false
    }

    fn has_tray(&self) -> bool {
        true
    }
}

fn rig(cx: &mut TestAppContext, pinned: bool, owner_repaired: bool) -> Rig {
    rig_with(cx, pinned, owner_repaired, false)
}

fn rig_with(
    cx: &mut TestAppContext,
    pinned: bool,
    owner_repaired: bool,
    keep_in_tray: bool,
) -> Rig {
    let fs = fake();
    let mut state = two_panes(&fs);
    state.find_memory_mut().pinned = pinned;
    let owner_calls = OwnerCalls::default();
    let pin_calls = PinCalls::default();
    let shown_calls = ShownCalls::default();
    let saves = Saves::default();
    let (sender, inbox) = open_requests();
    let mut options = options_for(fs, state);
    let recorded_owners = owner_calls.clone();
    let recorded_pins = pin_calls.clone();
    let recorded_shown = shown_calls.clone();
    let recorded_saves = saves.clone();
    options.on_save = Rc::new(move |state| {
        recorded_saves
            .borrow_mut()
            .push(state.find_memory().queries().len());
    });
    options.open.inbox = Some(inbox);
    options.open.resident = Some(Rc::new(TrayResident));
    options.open.window_shown = Some(Arc::new(move |handle, shown| {
        recorded_shown
            .lock()
            .expect("shown calls")
            .push((handle.map(raw_tag), shown));
        true
    }));
    options.settings.values = options.settings.values.with(&Change {
        field: Field::KeepInTray,
        value: Value::Flag(keep_in_tray),
    });
    options.search.find = FindHooks {
        pin: Arc::new(move |handle, on| {
            recorded_pins
                .lock()
                .expect("pin calls")
                .push((raw_tag(handle), on));
        }),
        ensure_owner: Arc::new(move |find_raw, navo_raw| {
            recorded_owners
                .lock()
                .expect("owner calls")
                .push((raw_tag(find_raw), raw_tag(navo_raw)));
            owner_repaired
        }),
        ..FindHooks::none()
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    Rig {
        window,
        view,
        owner_calls,
        pin_calls,
        shown_calls,
        sender,
        saves,
    }
}

impl Rig {
    fn open_find(&self, cx: &mut TestAppContext) {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| view.open_find(window, cx));
        })
        .expect("navo window");
        cx.run_until_parked();
    }

    fn close_find(&self, cx: &mut TestAppContext) {
        self.view.update(cx, |view, cx| view.close_find(cx));
        cx.run_until_parked();
    }

    fn find_window(&self, cx: &mut TestAppContext) -> Option<AnyWindowHandle> {
        self.view.read_with(cx, |view, _| view.find.window_handle())
    }

    fn shown_calls(&self) -> Vec<(Option<String>, bool)> {
        self.shown_calls.lock().expect("shown calls").clone()
    }

    fn tray_hide(&self, cx: &mut TestAppContext) -> bool {
        VisualTestContext::from_window(self.window, cx).simulate_close()
    }

    fn tray_show(&self, cx: &mut TestAppContext) {
        assert!(
            self.sender.send(OpenRequest::default()),
            "inbox takes arrivals"
        );
        settle(cx, &self.view);
    }

    fn hide_find(&self, cx: &mut TestAppContext) {
        let window = self.window;
        self.view.update(cx, |view, cx| view.hide_find(window, cx));
        cx.run_until_parked();
    }

    fn is_navo_active(&self, cx: &mut TestAppContext) -> bool {
        cx.update_window(self.window, |_, window, _| window.is_window_active())
            .expect("navo window")
    }

    fn add_tab(&self, cx: &mut TestAppContext, text: &str) -> u64 {
        self.view.update(cx, |view, cx| {
            view.find.book().update(cx, |book, _| {
                book.add(text, Location::new("/tmp/find").expect("test location"))
            })
        })
    }
}

/// Fills one tab's store with a single file holding `lines` hit lines.
fn fill_hits(book: &mut FindBook, id: u64, lines: u64) {
    let tab = book.tab_mut(id).expect("tab");
    let hits = (1..=lines)
        .map(|line| LineHit {
            line,
            column: 1,
            text: "a matched hit line with enough bytes for memory tests".into(),
        })
        .collect();
    let file = FoundFile {
        relative: ProviderPath::from_text("file.rs"),
        entry: FsEntry {
            name: ProviderPath::from_text("file.rs"),
            kind: EntryKind::File,
            size: 16,
            modified_ms: Some(1_000),
            attributes: Attributes::default(),
        },
        hits: FileHits {
            hits,
            total: u32::try_from(lines).unwrap_or(u32::MAX),
            skipped: None,
        },
    };
    let batch = FoundBatch {
        files: vec![file],
        progress: FindProgress::default(),
    };
    tab.store.push(0, &batch).expect("synthetic batch fits");
}

#[gpui_kit::test]
fn opening_creates_one_window_and_second_open_focuses_it(cx: &mut TestAppContext) {
    let rig = rig(cx, false, false);
    assert_eq!(cx.windows().len(), 1);

    rig.open_find(cx);
    assert_eq!(cx.windows().len(), 2);
    let first = rig.find_window(cx).expect("find window handle");

    VisualTestContext::from_window(first, cx).deactivate_window();
    cx.run_until_parked();
    let active_before = cx
        .update_window(first, |_, window, _| window.is_window_active())
        .expect("find window");
    assert!(!active_before);

    rig.open_find(cx);
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(rig.find_window(cx), Some(first));
    let active_after = cx
        .update_window(first, |_, window, _| window.is_window_active())
        .expect("find window");
    assert!(active_after);
}

#[gpui_kit::test]
fn close_find_removes_window_and_saves_placement_into_find_memory(cx: &mut TestAppContext) {
    let rig = rig(cx, false, false);
    rig.open_find(cx);
    let find_handle = rig.find_window(cx).expect("find window handle");
    cx.simulate_window_resize(find_handle, size(px(640.), px(420.)));
    cx.run_until_parked();
    let expected = cx
        .update_window(find_handle, |_, window, _| window.bounds())
        .expect("find window");

    rig.close_find(cx);
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(rig.find_window(cx), None);

    let (saved_len, save_requested) = rig.view.update(cx, |view, _| {
        let len = view.browser.state().find_memory().placement().len();
        let requested = view.browser.take_save_request();
        (len, requested)
    });
    assert_eq!(saved_len, 1);
    assert!(save_requested);

    rig.open_find(cx);
    let reopened = rig.find_window(cx).expect("reopened find window");
    let restored = cx
        .update_window(reopened, |_, window, _| window.bounds())
        .expect("reopened bounds");
    assert_eq!(restored, expected);
}

#[gpui_kit::test]
fn ensure_owner_hook_is_called_once_on_open_with_find_and_navo_handles(cx: &mut TestAppContext) {
    let rig = rig(cx, false, true);
    rig.open_find(cx);
    let find_handle = rig.find_window(cx).expect("find window handle");

    let calls = rig.owner_calls.lock().expect("owner calls").clone();
    assert_eq!(
        calls,
        vec![(
            raw_tag(test_raw_handle(find_handle)),
            raw_tag(test_raw_handle(rig.window))
        )]
    );
    assert_ne!(calls[0].0, calls[0].1);

    rig.open_find(cx);
    assert_eq!(rig.owner_calls.lock().expect("owner calls").len(), 1);
}

#[gpui_kit::test]
fn pin_hook_is_invoked_when_find_memory_pinned_is_true(cx: &mut TestAppContext) {
    let rig = rig(cx, true, false);
    rig.open_find(cx);
    let find_handle = rig.find_window(cx).expect("find window handle");

    let calls = rig.pin_calls.lock().expect("pin calls").clone();
    assert_eq!(calls, vec![(raw_tag(test_raw_handle(find_handle)), true)]);
}

#[gpui_kit::test]
fn pin_hook_is_not_invoked_when_find_memory_pinned_is_false(cx: &mut TestAppContext) {
    let rig = rig(cx, false, false);
    rig.open_find(cx);
    assert!(rig.pin_calls.lock().expect("pin calls").is_empty());
}

#[gpui_kit::test]
fn first_placement_opens_over_the_half_not_holding_the_focused_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, false, false);
    let panes = rig.view.read_with(cx, |view, _| view.panes());
    assert_eq!(panes.len(), 2);

    // Focus the left pane -> find window opens over the right half (x >= 500, size 450x480).
    rig.view.update(cx, |view, _| view.focus_pane(panes[0]));
    rig.open_find(cx);
    let right_handle = rig.find_window(cx).expect("right find window");
    let right_bounds = cx
        .update_window(right_handle, |_, window, _| window.bounds())
        .expect("right bounds");
    assert!(right_bounds.origin.x >= px(500.), "{right_bounds:?}");
    assert_eq!(right_bounds.size, size(px(450.), px(480.)));

    // Clear saved placement, focus the right pane -> find window opens over the left half.
    rig.close_find(cx);
    rig.view.update(cx, |view, _| {
        *view.browser.find_memory_mut() = navo_core::FindMemory::default();
        view.focus_pane(panes[1]);
    });
    rig.open_find(cx);
    let left_handle = rig.find_window(cx).expect("left find window");
    let left_bounds = cx
        .update_window(left_handle, |_, window, _| window.bounds())
        .expect("left bounds");
    assert!(left_bounds.origin.x < px(500.), "{left_bounds:?}");
    assert_eq!(left_bounds.size, size(px(450.), px(480.)));
}

#[gpui_kit::test]
fn tray_hide_and_show_call_the_find_hook_before_navo(cx: &mut TestAppContext) {
    let rig = rig_with(cx, false, false, true);
    rig.open_find(cx);
    let find = rig.find_window(cx).expect("find window handle");
    let tag = Some(raw_tag(test_raw_handle(find)));

    assert!(!rig.tray_hide(cx));
    assert_eq!(rig.shown_calls(), vec![(tag.clone(), false), (None, false)]);

    rig.tray_show(cx);
    assert_eq!(
        rig.shown_calls(),
        vec![
            (tag.clone(), false),
            (None, false),
            (tag, true),
            (None, true)
        ]
    );
}

#[gpui_kit::test]
fn minimise_hides_find_hands_focus_to_navo_and_tray_skips_it(cx: &mut TestAppContext) {
    let rig = rig_with(cx, false, false, true);
    rig.open_find(cx);
    let find = rig.find_window(cx).expect("find window handle");
    let tag = Some(raw_tag(test_raw_handle(find)));
    VisualTestContext::from_window(find, cx).deactivate_window();
    cx.run_until_parked();

    rig.hide_find(cx);
    assert_eq!(rig.shown_calls(), vec![(tag.clone(), false)]);
    assert!(rig.view.read_with(cx, |view, _| view.find.is_hidden()));
    assert!(rig.is_navo_active(cx));

    assert!(!rig.tray_hide(cx));
    rig.tray_show(cx);
    assert_eq!(
        rig.shown_calls(),
        vec![(tag, false), (None, false), (None, true)]
    );
}

#[gpui_kit::test]
fn reopen_while_user_hidden_shows_and_focuses_the_same_window(cx: &mut TestAppContext) {
    let rig = rig_with(cx, false, false, true);
    rig.open_find(cx);
    let find = rig.find_window(cx).expect("find window handle");
    let tag = Some(raw_tag(test_raw_handle(find)));
    rig.hide_find(cx);
    VisualTestContext::from_window(find, cx).deactivate_window();
    cx.run_until_parked();

    rig.open_find(cx);
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(rig.find_window(cx), Some(find));
    assert!(!rig.view.read_with(cx, |view, _| view.find.is_hidden()));
    assert_eq!(rig.shown_calls(), vec![(tag.clone(), false), (tag, true)]);
    let active = cx
        .update_window(find, |_, window, _| window.is_window_active())
        .expect("find window");
    assert!(active);
}

#[gpui_kit::test]
fn find_host_restores_saved_queries_as_idle_tabs(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = two_panes(&fs);
    let base = fs.location("home/docs");
    state.find_memory_mut().set_queries(vec![
        SavedFind {
            text: "report".into(),
            base: base.clone(),
        },
        SavedFind {
            text: "needle ext:rs".into(),
            base,
        },
    ]);
    let (_window, view) = open_options(cx, options_for(fs, state));
    let book = view.read_with(cx, |view, _| view.find.book().clone());
    book.read_with(cx, |book, _| {
        assert_eq!(book.len(), 2);
        assert_eq!(book.tabs()[0].text, "report");
        assert_eq!(book.tabs()[1].text, "needle ext:rs");
        assert!(book.tabs().iter().all(|tab| !tab.is_running()));
    });
}

#[gpui_kit::test]
fn navo_close_persists_find_queries_before_the_session_save(cx: &mut TestAppContext) {
    let rig = rig(cx, false, false);
    rig.open_find(cx);
    rig.add_tab(cx, "needle");

    assert!(rig.tray_hide(cx));
    let queries = rig.view.read_with(cx, |view, _| {
        view.browser.state().find_memory().queries().to_vec()
    });
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].text, "needle");
    assert_eq!(rig.saves.borrow().last().copied(), Some(1));
}

#[gpui_kit::test]
fn user_close_activates_navo_persists_and_trims(cx: &mut TestAppContext) {
    let rig = rig(cx, true, false);
    rig.open_find(cx);
    let find = rig.find_window(cx).expect("find window handle");
    let first = rig.add_tab(cx, "alpha");
    let second = rig.add_tab(cx, "beta");
    rig.view.update(cx, |view, cx| {
        view.find.book().update(cx, |book, _| {
            fill_hits(book, first, 8);
            fill_hits(book, second, 8);
        });
    });
    VisualTestContext::from_window(find, cx).deactivate_window();
    cx.run_until_parked();

    assert!(VisualTestContext::from_window(find, cx).simulate_close());
    cx.run_until_parked();
    assert!(rig.is_navo_active(cx));
    assert_eq!(rig.find_window(cx), None);
    let (queries, pinned) = rig.view.read_with(cx, |view, _| {
        let memory = view.browser.state().find_memory();
        (memory.queries().to_vec(), memory.pinned)
    });
    assert_eq!(
        queries
            .iter()
            .map(|query| query.text.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert!(pinned);
    rig.view.update(cx, |view, cx| {
        view.find.book().update(cx, |book, _| {
            for id in [first, second] {
                let tab = book.tab(id).expect("tab");
                assert_eq!(tab.store.file(0).expect("file").lines().count(), 3);
            }
        });
    });
}
