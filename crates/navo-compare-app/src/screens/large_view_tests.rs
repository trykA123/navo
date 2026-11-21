//! View tests of the Large screen: the labels it shows and the `Enter` key.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::test_support::texts;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Focusable as _, TestAppContext};
use sidewise_kit::tokens::Mode;
use sidewise_ui::browser::{ComparePhase, CompareTotals, CompareUpdate, SlowPath};

use super::large_model::PhaseMeters;
use super::large_view::{LargeEvent, LargeSnapshot, LargeView, bind_keys};
use crate::test_window::open;
use crate::theme::apply_sidewise;
use navo_compare_profile::appearance::PaletteId;

/// Opens the view with the keys bound and the Sidewise theme applied.
fn view(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<LargeView>) {
    open(cx, |_, cx| {
        bind_keys(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
        LargeView::new(cx)
    })
}

/// A snapshot with Listing 3/None and QuickCheck 5/10.
fn snapshot(checking: bool) -> LargeSnapshot {
    let mut meters = PhaseMeters::default();
    for (phase, done, total) in [
        (ComparePhase::Listing, 3, None),
        (ComparePhase::QuickCheck, 5, Some(10)),
    ] {
        meters.observe(&CompareUpdate::PhaseProgress {
            generation: 1,
            phase,
            done,
            total,
        });
    }
    LargeSnapshot {
        meters,
        totals: CompareTotals::default(),
        slow: Vec::new(),
        reading: None,
        checking,
    }
}

/// Every text the window draws.
fn shown(cx: &mut TestAppContext, w: AnyWindowHandle) -> Vec<String> {
    cx.update_window(w, |_, window, _| {
        texts(window).iter().map(|t| t.text().to_string()).collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn snapshot_renders_labels_and_status(cx: &mut TestAppContext) {
    let (w, v) = view(cx);
    cx.update_window(w, |_, _, cx| {
        v.update(cx, |v, cx| v.set_snapshot(snapshot(true), cx))
    })
    .expect("window");
    let all = shown(cx, w);
    assert!(all.iter().any(|t| t == "Size and time"), "{all:?}");
    assert!(all.iter().any(|t| t == "Comparing"));
    cx.update_window(w, |_, _, cx| {
        v.update(cx, |v, cx| v.set_snapshot(snapshot(false), cx))
    })
    .expect("window");
    assert!(shown(cx, w).iter().any(|t| t == "Finished"));
}

#[gpui_kit::test]
fn enter_emits_show_folders(cx: &mut TestAppContext) {
    let (w, v) = view(cx);
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    cx.update(|cx| {
        cx.subscribe(&v, move |_, event: &LargeEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });
    cx.update_window(w, |_, window, cx| v.focus_handle(cx).focus(window, cx))
        .expect("window");
    cx.simulate_keystrokes(w, "enter");
    assert_eq!(*log.borrow(), [LargeEvent::ShowFolders]);
}

#[gpui_kit::test]
fn a_long_running_read_shows_its_path_and_age(cx: &mut TestAppContext) {
    let (w, v) = view(cx);
    let reading = LargeSnapshot {
        reading: Some(SlowPath {
            path: "crates/big.bin".to_owned(),
            elapsed_ms: 6200,
        }),
        ..snapshot(true)
    };
    cx.update_window(w, |_, _, cx| {
        v.update(cx, |v, cx| v.set_snapshot(reading, cx))
    })
    .expect("window");
    let all = shown(cx, w);
    assert!(
        all.iter().any(|t| t == "Reading crates/big.bin · 6.2 s"),
        "{all:?}"
    );
}
