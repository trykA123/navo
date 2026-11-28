use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, TestAppContext, px, size,
};
use navo_core::find::{FileHits, LineHit};
use navo_core::{EntryKind, Location, ProviderPath};
use navo_fs::find::{FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use super::super::find_window::test_raw_handle;
use super::super::{FindHooks, NavoWindow, RawWindowHandle};
use super::{fake, open_options, options_for, settle, two_panes};
use crate::find_results::book::FindBook;
use crate::test_pointer::{click, dispatch, down};

type PinCalls = Arc<Mutex<Vec<(String, bool)>>>;
type ShownCalls = Arc<Mutex<Vec<(Option<String>, bool)>>>;

fn raw_tag(handle: RawWindowHandle) -> String {
    format!("{handle:?}")
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    pin_calls: PinCalls,
    shown_calls: ShownCalls,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let pin_calls = PinCalls::default();
    let shown_calls = ShownCalls::default();
    let mut options = options_for(fs, state);
    let recorded_pins = pin_calls.clone();
    let recorded_shown = shown_calls.clone();
    options.open.window_shown = Some(Arc::new(move |handle, shown| {
        recorded_shown
            .lock()
            .expect("shown calls")
            .push((handle.map(raw_tag), shown));
        true
    }));
    options.search.find = FindHooks {
        pin: Arc::new(move |handle, on| {
            recorded_pins
                .lock()
                .expect("pin calls")
                .push((raw_tag(handle), on));
        }),
        ..FindHooks::none()
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    Rig {
        window,
        view,
        pin_calls,
        shown_calls,
    }
}

impl Rig {
    fn open_find(&self, cx: &mut TestAppContext) -> AnyWindowHandle {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| view.open_find(window, cx));
        })
        .expect("navo window");
        cx.run_until_parked();
        let handle = self
            .view
            .read_with(cx, |view, _| view.find.window_handle())
            .expect("find window handle");
        cx.simulate_window_resize(handle, size(px(640.), px(420.)));
        cx.run_until_parked();
        handle
    }

    fn add_tab(&self, cx: &mut TestAppContext, text: &str) -> u64 {
        self.view.update(cx, |view, cx| {
            view.find.book().update(cx, |book, cx| {
                let id = book.add(text, Location::new("/tmp/find").expect("test location"));
                cx.notify();
                id
            })
        })
    }

    fn select_tab(&self, cx: &mut TestAppContext, id: u64) {
        self.view.update(cx, |view, cx| {
            view.find.book().update(cx, |book, cx| {
                book.select(id);
                cx.notify();
            });
        });
    }

    fn active_id(&self, cx: &TestAppContext) -> Option<u64> {
        self.view.read_with(cx, |view, cx| {
            view.find.book().read(cx).active().map(|tab| tab.id)
        })
    }

    fn tab_ids(&self, cx: &TestAppContext) -> Vec<u64> {
        self.view.read_with(cx, |view, cx| {
            view.find
                .book()
                .read(cx)
                .tabs()
                .iter()
                .map(|tab| tab.id)
                .collect()
        })
    }

    fn pinned(&self, cx: &TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.browser.state().find_memory().pinned)
    }

    fn pin_calls(&self) -> Vec<(String, bool)> {
        self.pin_calls.lock().expect("pin calls").clone()
    }

    fn shown_calls(&self) -> Vec<(Option<String>, bool)> {
        self.shown_calls.lock().expect("shown calls").clone()
    }
}

fn fill_hits(book: &mut FindBook, id: u64, lines: u64) {
    let tab = book.tab_mut(id).expect("tab");
    let hits = (1..=lines)
        .map(|line| LineHit {
            line,
            column: 1,
            text: "matched line".into(),
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
    tab.store.push(0, &batch).expect("batch fits");
}

#[gpui_kit::test]
fn clicking_second_tab_switches_active_tab(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let first = rig.add_tab(cx, "alpha");
    let second = rig.add_tab(cx, "beta");
    rig.select_tab(cx, first);
    let find = rig.open_find(cx);
    assert_eq!(rig.active_id(cx), Some(first));

    let at = cx
        .update_window(find, |_, window, cx| {
            window.render_frame(cx);
            window.find(("find-tab", second)).bounds().center()
        })
        .expect("find window");
    click(cx, find, (at, Modifiers::default()));
    cx.run_until_parked();

    assert_eq!(rig.active_id(cx), Some(second));
}

#[gpui_kit::test]
fn close_button_and_middle_click_close_target_tab(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let first = rig.add_tab(cx, "alpha");
    let second = rig.add_tab(cx, "beta");
    let third = rig.add_tab(cx, "gamma");
    rig.view.update(cx, |view, cx| {
        view.find.book().update(cx, |book, cx| {
            fill_hits(book, second, 4);
            cx.notify();
        });
    });
    let find = rig.open_find(cx);

    let close_at = cx
        .update_window(find, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find(("find-tab-hits", second)).visible());
            window.find(("find-tab-close", second)).bounds().center()
        })
        .expect("find window");
    click(cx, find, (close_at, Modifiers::default()));
    cx.run_until_parked();
    assert_eq!(rig.tab_ids(cx), vec![first, third]);

    let mid_at = cx
        .update_window(find, |_, window, cx| {
            window.render_frame(cx);
            window.find(("find-tab", first)).bounds().center()
        })
        .expect("find window");
    dispatch(
        cx,
        find,
        down(MouseButton::Middle, mid_at, Modifiers::default()),
    );
    cx.run_until_parked();
    assert_eq!(rig.tab_ids(cx), vec![third]);
}

#[gpui_kit::test]
fn add_button_opens_new_empty_tab(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let first = rig.add_tab(cx, "alpha");
    let find = rig.open_find(cx);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        window.click("find-tab-add", cx);
    })
    .expect("find window");
    cx.run_until_parked();

    let ids = rig.tab_ids(cx);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], first);
    assert_eq!(rig.active_id(cx), Some(ids[1]));
    let text = rig.view.read_with(cx, |view, cx| {
        view.find
            .book()
            .read(cx)
            .active()
            .map(|tab| tab.text.clone())
    });
    assert_eq!(text.as_deref(), Some(""));
}

#[gpui_kit::test]
fn pin_button_flips_find_memory_and_invokes_pin_hook(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(cx, "alpha");
    let find = rig.open_find(cx);
    let find_tag = raw_tag(test_raw_handle(find));
    assert!(!rig.pinned(cx));

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        window.click("find-window-pin", cx);
    })
    .expect("find window");
    cx.run_until_parked();
    assert!(rig.pinned(cx));
    assert_eq!(rig.pin_calls(), vec![(find_tag.clone(), true)]);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        window.click("find-window-pin", cx);
    })
    .expect("find window");
    cx.run_until_parked();
    assert!(!rig.pinned(cx));
    assert_eq!(
        rig.pin_calls(),
        vec![(find_tag.clone(), true), (find_tag, false)]
    );
}

#[gpui_kit::test]
fn minimise_button_hides_find_window_and_marks_hidden(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(cx, "alpha");
    let find = rig.open_find(cx);
    let find_tag = Some(raw_tag(test_raw_handle(find)));

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        window.click("title-bar-minimize", cx);
    })
    .expect("find window");
    cx.run_until_parked();

    assert_eq!(rig.shown_calls(), vec![(find_tag, false)]);
    assert!(rig.view.read_with(cx, |view, _| view.find.is_hidden()));
}
