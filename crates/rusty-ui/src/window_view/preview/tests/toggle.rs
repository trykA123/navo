use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;

use gpui_kit::test::TestWindowExt;

use super::{Rig, open_fake};
use crate::primitives::Kbd;
use crate::window_view::preview::Shown;
use crate::window_view::tests::fake;

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.read(cx, |view| view.preview.is_open())
}

#[gpui_kit::test]
fn ctrl_q_neither_opens_nor_closes_the_preview(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    rig.key(cx, "ctrl-q");
    assert!(!is_open(cx, &rig));
    rig.key(cx, "f3");
    assert!(is_open(cx, &rig));
    rig.key(cx, "ctrl-q");
    assert!(is_open(cx, &rig));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn f3_opens_the_preview_and_it_follows_the_cursor_onto_a_file(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    rig.key(cx, "f3");
    assert!(is_open(cx, &rig));
    rig.settle_motion(cx);
    assert!(rig.shows(cx, "preview-sheet"));
    assert_eq!(rig.shown_name(cx).as_deref(), Some("docs"));
    rig.key(cx, "down");
    assert!(rig.rest_until(cx, |view| {
        matches!(view.preview.shown(), Some(Shown::File(file)) if file.target.name() == "a.txt")
    }));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn escape_closes_the_preview_opened_with_f3(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    rig.key(cx, "f3");
    rig.settle_motion(cx);
    rig.key(cx, "escape");
    assert!(!is_open(cx, &rig));
    rig.settle_motion(cx);
    assert!(!rig.shows(cx, "preview-sheet"));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn no_card_has_a_preview_button(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    let panes = rig.read(cx, |view| view.panes());
    for pane in panes {
        assert!(!rig.shows(cx, ("preview-button", pane.get())));
    }
    rig.key(cx, "f3");
    rig.settle_motion(cx);
    let active = rig.active_pane(cx);
    assert!(!rig.shows(cx, ("preview-button", active)));
}

#[gpui_kit::test]
fn the_open_state_outlives_the_viewer(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    rig.open(cx);
    rig.key(cx, "down");
    rig.key(cx, "f3");
    assert!(rig.read(cx, |view| view.viewer.is_some()));
    assert!(!rig.shows(cx, "preview-sheet"));
    rig.key(cx, "escape");
    assert!(rig.read(cx, |view| view.viewer.is_none()));
    assert!(is_open(cx, &rig));
    rig.settle_motion(cx);
    assert!(rig.shows(cx, "preview-sheet"));
}

#[gpui_kit::test]
fn the_footer_names_every_preview_key(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    rig.open(cx);
    assert!(rig.wait_until(cx, |view| {
        view.preview
            .folder()
            .is_some_and(|folder| !folder.scan.is_counting())
    }));
    let hints: Vec<String> = (0..5usize)
        .filter_map(|index| rig.label(cx, ("side-sheet-hint", index)))
        .collect();
    assert_eq!(hints, ["Browse", "Open", "[ ]", "Close"]);
    let escape = Kbd::new("escape", "escape").map(|kbd| kbd.text());
    let close_keys = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            let hint = window
                .within(("side-sheet-hint", 3usize))
                .find(("key", 0usize));
            let button = window.find("side-sheet-close-key");
            [
                hint.label().map(str::to_owned),
                button.label().map(str::to_owned),
            ]
        })
        .expect("window");
    assert_eq!(close_keys, [escape.clone(), escape]);
}
