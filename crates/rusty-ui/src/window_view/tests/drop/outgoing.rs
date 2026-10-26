use gpui_kit::{ExternalPaths, FileDropEvent};
use std::path::PathBuf;

use super::external::rig_platform;
use super::*;

fn outside() -> Point<Pixels> {
    point(px(-12.), px(140.))
}

#[gpui_kit::test]
fn outgoing_drag_offers_frozen_marked_paths_and_folder_flags(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    rig.mark(cx, &[DOCS, A]);
    let to = outside();
    rig.drag_to(cx, (A, to), Modifiers::default());
    let files = platform.offered.borrow().clone();
    assert_eq!(
        files,
        [
            (PathBuf::from(rig.fs.location("home/docs").as_str()), true),
            (PathBuf::from(rig.fs.location("home/a.txt").as_str()), false),
        ]
    );
    cx.update_window(rig.window, |_, window, cx| {
        cx.stop_active_drag(window);
    })
    .expect("suspend source drag");
    assert!(!rig.has_drag(cx));
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("render");
    assert!(rig.has_session(cx));
    dispatch(cx, rig.window, FileDropEvent::Ended.to_platform_input());
    assert!(!rig.has_session(cx));
    assert!(rig.jobs(cx).is_empty());
}

#[gpui_kit::test]
fn reentering_after_offering_native_payload_uses_internal_modifiers(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    rig.drag_to(cx, (A, outside()), Modifiers::default());
    platform.keys.set(rusty_core::DropKeys {
        ctrl: true,
        ..rusty_core::DropKeys::default()
    });
    let to = rig.list_spot(cx, rig.dest);
    dispatch(
        cx,
        rig.window,
        FileDropEvent::Entered {
            position: to,
            paths: ExternalPaths([PathBuf::from("home/a.txt")].into_iter().collect()),
        }
        .to_platform_input(),
    );
    assert!(rig.has_drag(cx));
    super::external::submit(&rig, cx, to);
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Move, Some(rig.fs.location("dest")))]
    );
    dispatch(cx, rig.window, FileDropEvent::Ended.to_platform_input());
    assert!(!rig.exists("home/a.txt"));
    assert!(rig.exists("dest/a.txt"));
}

#[gpui_kit::test]
fn unsupported_sources_offer_no_external_payload(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.offer.set(false);
    rig.drag_to(cx, (A, outside()), Modifiers::default());
    assert!(platform.offered.borrow().is_empty());
    assert!(rig.has_drag(cx));
    rig.key(cx, "escape");
    assert!(!rig.has_session(cx));
    assert!(rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn native_drag_end_refreshes_an_unwatched_source_without_deleting(cx: &mut TestAppContext) {
    let (rig, _) = rig_platform(cx);
    rig.drag_to(cx, (A, outside()), Modifiers::default());
    rig.fs.remove("home/a.txt");
    dispatch(cx, rig.window, FileDropEvent::Ended.to_platform_input());
    settle(cx, &rig.view);
    let names = rig.view.read_with(cx, |view, _| {
        view.browser
            .state()
            .pane(rig.home)
            .expect("pane")
            .active()
            .listing()
            .expect("listing")
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>()
    });
    assert!(!names.iter().any(|name| name == "a.txt"));
    assert!(rig.exists("home/b.txt"));
    assert!(rig.jobs(cx).is_empty());
}

#[gpui_kit::test]
fn a_refused_platform_handoff_cleans_up_after_mouse_release(cx: &mut TestAppContext) {
    let (rig, _) = rig_platform(cx);
    rig.drag_to(cx, (A, outside()), Modifiers::default());
    rig.release(cx, outside(), Modifiers::default());
    assert!(!rig.has_session(cx));
    assert!(rig.jobs(cx).is_empty());
}

#[gpui_kit::test]
fn a_source_listing_refresh_does_not_change_the_native_payload(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    let inside = rig.list_spot(cx, rig.dest);
    rig.drag_to(cx, (A, inside), Modifiers::default());
    rig.fs.remove("home/a.txt");
    rig.refresh(cx);
    dispatch(
        cx,
        rig.window,
        moved(Some(MouseButton::Left), outside(), Modifiers::default()),
    );
    assert_eq!(
        platform.offered.borrow().as_slice(),
        &[(PathBuf::from(rig.fs.location("home/a.txt").as_str()), false)]
    );
}

#[gpui_kit::test]
fn returning_internal_drag_allows_move_after_a_copy_only_native_offer(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.allowed.set([true, false, false]);
    rig.drag_to(cx, (A, outside()), Modifiers::default());
    let to = rig.list_spot(cx, rig.dest);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    dispatch(cx, rig.window, moved(Some(MouseButton::Left), to, shift));
    rig.release(cx, to, shift);
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Move, Some(rig.fs.location("dest")))]
    );
    assert!(!rig.exists("home/a.txt"));
    assert!(rig.exists("dest/a.txt"));
}
