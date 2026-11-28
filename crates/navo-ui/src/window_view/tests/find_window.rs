use std::sync::{Arc, Mutex};

use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, px, size,
};

use super::super::find_window::test_raw_handle;
use super::super::{FindHooks, NavoWindow, RawWindowHandle};
use super::{fake, open_options, options_for, settle, two_panes};

type OwnerCalls = Arc<Mutex<Vec<(String, String)>>>;
type PinCalls = Arc<Mutex<Vec<(String, bool)>>>;

fn raw_tag(handle: RawWindowHandle) -> String {
    format!("{handle:?}")
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    owner_calls: OwnerCalls,
    pin_calls: PinCalls,
}

fn rig(cx: &mut TestAppContext, pinned: bool, owner_repaired: bool) -> Rig {
    let fs = fake();
    let mut state = two_panes(&fs);
    state.find_memory_mut().pinned = pinned;
    let owner_calls = OwnerCalls::default();
    let pin_calls = PinCalls::default();
    let mut options = options_for(fs, state);
    let recorded_owners = owner_calls.clone();
    let recorded_pins = pin_calls.clone();
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
