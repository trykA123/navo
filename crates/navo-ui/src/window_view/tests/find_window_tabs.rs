use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, Role, TestAppContext, px,
    size,
};
use navo_core::find::{FileHits, LineHit};
use navo_core::{EntryKind, Location, ProviderPath};
use navo_fs::find::{FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use super::super::find_window::test_raw_handle;
use super::super::{FindHooks, NavoWindow, RawWindowHandle};
use super::{fake, open_options, options_for, settle, two_panes};
use crate::find_results::book::{FindBook, MAX_TABS};
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
    let saved_pins = Arc::new(Mutex::new(Vec::new()));
    let sink = saved_pins.clone();
    rig.view.update(cx, |view, _| {
        view.autosave.save = Some(Arc::new(move |state| {
            sink.lock()
                .expect("saved pins")
                .push(state.find_memory().pinned);
        }));
    });
    let _ = rig.add_tab(cx, "alpha");
    let find = rig.open_find(cx);
    let find_tag = raw_tag(test_raw_handle(find));
    assert!(!rig.pinned(cx));

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        let pin = window.find("find-window-pin");
        assert_eq!(pin.label(), Some("Keep above other windows"));
        assert_eq!(pin.description(), Some("Keep above other windows"));
        window.click("find-window-pin", cx);
    })
    .expect("find window");
    cx.background_executor.advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert!(rig.pinned(cx));
    assert_eq!(rig.pin_calls(), vec![(find_tag.clone(), true)]);
    assert_eq!(*saved_pins.lock().expect("saved pins"), vec![true]);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        let pin = window.find("find-window-pin");
        assert_eq!(pin.label(), Some("Stop keeping above other windows"));
        assert_eq!(pin.description(), Some("Stop keeping above other windows"));
        window.click("find-window-pin", cx);
    })
    .expect("find window");
    cx.background_executor.advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert!(!rig.pinned(cx));
    assert_eq!(
        rig.pin_calls(),
        vec![(find_tag.clone(), true), (find_tag, false)]
    );
    assert_eq!(*saved_pins.lock().expect("saved pins"), vec![true, false]);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        let pin = window.find("find-window-pin");
        assert_eq!(pin.label(), Some("Keep above other windows"));
        assert_eq!(pin.description(), Some("Keep above other windows"));
    })
    .expect("find window");
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

#[gpui_kit::test]
fn title_bar_renders_at_36px_and_tab_strip_exposes_a11y_label(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(cx, "alpha");
    let find = rig.open_find(cx);

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.find("title-bar");
        assert_eq!(bar.bounds().size.height, px(36.));
        let strip = window.find("find-tab-strip");
        assert_eq!(strip.role(), Some(Role::TabList));
        assert_eq!(strip.label(), Some("Tabs"));
    })
    .expect("find window");
}

#[gpui_kit::test]
fn twelve_tabs_at_420px_stay_within_brand_and_right_controls(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let mut ids = Vec::with_capacity(MAX_TABS);
    for idx in 0..MAX_TABS {
        let id = rig.add_tab(cx, &format!("search-query-{idx:02}"));
        ids.push(id);
    }
    rig.view.update(cx, |view, cx| {
        view.find.book().update(cx, |book, cx| {
            fill_hits(book, ids[0], 12);
            cx.notify();
        });
    });
    let find = rig.open_find(cx);
    cx.simulate_window_resize(find, size(px(420.), px(300.)));
    cx.run_until_parked();

    cx.update_window(find, |_, window, cx| {
        window.render_frame(cx);
        let brand = window.find("title-bar-title").bounds();
        let header = window.find("title-bar-header").bounds();
        let strip = window.find("find-tab-strip").bounds();
        let add = window.find("find-tab-add").bounds();
        let pin = window.find("find-window-pin").bounds();
        let minimize = window.find("title-bar-minimize").bounds();

        assert!(
            header.left() >= brand.right(),
            "title-bar-header left {:?} overlaps brand right {:?}",
            header.left(),
            brand.right()
        );
        assert!(
            header.right() <= pin.left(),
            "title-bar-header right {:?} overlaps pin left {:?}",
            header.right(),
            pin.left()
        );
        assert!(
            strip.left() >= brand.right(),
            "tab strip left {:?} overlaps brand right {:?}",
            strip.left(),
            brand.right()
        );
        assert!(
            strip.right() <= pin.left(),
            "tab strip right {:?} overlaps pin left {:?}",
            strip.right(),
            pin.left()
        );
        assert!(
            add.right() <= strip.right(),
            "add button right {:?} overflows strip right {:?}",
            add.right(),
            strip.right()
        );
        assert!(pin.right() <= minimize.left());

        for id in &ids {
            let tab = window.find(("find-tab", *id)).bounds();
            assert!(
                tab.left() >= strip.left(),
                "tab {id} left {:?} < strip left {:?}",
                tab.left(),
                strip.left()
            );
            assert!(
                tab.right() <= add.left(),
                "tab {id} right {:?} > add left {:?}",
                tab.right(),
                add.left()
            );
            assert!(
                tab.right() <= pin.left(),
                "tab {id} right {:?} overlaps pin left {:?}",
                tab.right(),
                pin.left()
            );
        }
    })
    .expect("find window");
}

#[gpui_kit::test]
fn closing_active_tab_selects_remaining_tab(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let first = rig.add_tab(cx, "alpha");
    let second = rig.add_tab(cx, "beta");
    let third = rig.add_tab(cx, "gamma");
    rig.select_tab(cx, second);
    let find = rig.open_find(cx);
    assert_eq!(rig.active_id(cx), Some(second));

    let close_at = cx
        .update_window(find, |_, window, cx| {
            window.render_frame(cx);
            window.find(("find-tab-close", second)).bounds().center()
        })
        .expect("find window");
    click(cx, find, (close_at, Modifiers::default()));
    cx.run_until_parked();

    let remaining = rig.tab_ids(cx);
    assert_eq!(remaining, vec![first, third]);
    let active = rig.active_id(cx);
    assert!(
        active.is_some_and(|id| remaining.contains(&id)),
        "expected an active remaining tab, got {active:?}"
    );
}

#[gpui_kit::test]
fn closing_last_remaining_tab_hides_find_window(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let only = rig.add_tab(cx, "solo");
    let find = rig.open_find(cx);
    let find_tag = Some(raw_tag(test_raw_handle(find)));

    let close_at = cx
        .update_window(find, |_, window, cx| {
            window.render_frame(cx);
            window.find(("find-tab-close", only)).bounds().center()
        })
        .expect("find window");
    click(cx, find, (close_at, Modifiers::default()));
    cx.run_until_parked();

    assert!(rig.tab_ids(cx).is_empty());
    assert_eq!(rig.shown_calls(), vec![(find_tag, false)]);
    assert!(rig.view.read_with(cx, |view, _| view.find.is_hidden()));
}

#[gpui_kit::test]
fn tabs_add_and_pin_occlude_title_bar_drag(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let first = rig.add_tab(cx, "alpha");
    let second = rig.add_tab(cx, "beta");
    rig.select_tab(cx, first);
    let find = rig.open_find(cx);

    let (bar_bounds, tab_at, add_at, pin_at) = cx
        .update_window(find, |_, window, cx| {
            window.render_frame(cx);
            let bar = window.find("title-bar").bounds();
            let tab = window.find(("find-tab", second)).bounds();
            let add = window.find("find-tab-add").bounds();
            let pin = window.find("find-window-pin").bounds();
            assert!(bar.contains(&tab.center()));
            assert!(bar.contains(&add.center()));
            assert!(bar.contains(&pin.center()));
            window.drag_to(("find-tab", first), ("find-tab", second), cx);
            window.double_click(("find-tab", first), cx);
            (bar, tab.center(), add.center(), pin.center())
        })
        .expect("find window");

    click(cx, find, (tab_at, Modifiers::default()));
    cx.run_until_parked();
    assert_eq!(rig.active_id(cx), Some(second));

    click(cx, find, (add_at, Modifiers::default()));
    cx.run_until_parked();
    assert_eq!(rig.tab_ids(cx).len(), 3);

    click(cx, find, (pin_at, Modifiers::default()));
    cx.run_until_parked();
    assert!(rig.pinned(cx));

    let after_bar = cx
        .update_window(find, |_, window, cx| {
            window.render_frame(cx);
            window.find("title-bar").bounds()
        })
        .expect("find window");
    assert_eq!(after_bar, bar_bounds);
}
