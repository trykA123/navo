use std::path::PathBuf;

use rusty_core::find::FindQuery;

use super::external::rig_platform;
use super::*;

const FOUND: [&str; 2] = ["deep/b.txt", "docs/report.txt"];

fn show_results(rig: &Rig, cx: &mut TestAppContext) {
    rig.fs.add_file("home/deep/b.txt", 4);
    rig.view.update(cx, |view, _| {
        view.browser.find_only(&FOUND);
        let base = rig.fs.location("home");
        view.browser
            .start_find(rig.home, FindQuery::parse("x"), base);
    });
    settle(cx, &rig.view);
    rig.mark(cx, &[0, 1]);
}

fn real(rig: &Rig) -> Vec<Location> {
    FOUND
        .iter()
        .map(|name| rig.fs.location(&format!("home/{name}")))
        .collect()
}

fn rows(rig: &Rig, cx: &mut TestAppContext) -> Vec<String> {
    rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().pane(rig.home).expect("pane").active();
        let listing = tab.listing().expect("results listing");
        listing.entries().iter().map(|e| e.name.clone()).collect()
    })
}

#[gpui_kit::test]
fn a_drag_from_results_carries_the_real_files_and_offers_them_outside(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    show_results(&rig, cx);
    rig.drag_to(cx, (1, point(px(-12.), px(140.))), Modifiers::default());
    let (sources, parents) = rig.view.read_with(cx, |view, _| {
        let session = view.drag.session.as_ref().expect("a drag");
        (
            session.carried.sources.clone(),
            session.carried.parents.clone(),
        )
    });
    assert_eq!(sources, real(&rig));
    assert_eq!(
        parents,
        [rig.fs.location("home/deep"), rig.fs.location("home/docs")]
    );
    let expected: Vec<PathBuf> = real(&rig)
        .iter()
        .map(|place| PathBuf::from(place.as_str()))
        .collect();
    let offered: Vec<PathBuf> = platform
        .offered
        .borrow()
        .iter()
        .map(|(path, _)| path.clone())
        .collect();
    assert_eq!(offered, expected);
    rig.key(cx, "escape");
}

#[gpui_kit::test]
fn a_drag_move_from_results_moves_the_real_files_and_their_rows_leave(cx: &mut TestAppContext) {
    let rig = rig(cx);
    show_results(&rig, cx);
    rig.second_tab(cx, rig.dest, "home/deep");
    assert_eq!(rows(&rig, cx), FOUND);
    let to = rig.list_spot(cx, rig.dest);
    rig.drop_on(cx, (1, to), Modifiers::default());
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Move, Some(rig.fs.location("dest")))]
    );
    assert!(rig.exists("dest/report.txt"));
    assert!(rig.exists("dest/b.txt"));
    assert!(!rig.exists("home/docs/report.txt"));
    assert!(!rig.exists("home/deep/b.txt"));
    settle(cx, &rig.view);
    assert!(rows(&rig, cx).is_empty());
}

fn footer(rig: &Rig, cx: &mut TestAppContext) -> String {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .find(("status", rig.home.get()))
            .label()
            .map(str::to_owned)
    })
    .expect("window")
    .expect("label")
}

#[gpui_kit::test]
fn the_footer_of_a_results_listing_follows_rows_that_leave(cx: &mut TestAppContext) {
    const THREE: [&str; 3] = ["a.txt", "deep/b.txt", "docs/report.txt"];
    let rig = rig(cx);
    rig.fs.add_file("home/deep/b.txt", 4);
    rig.view.update(cx, |view, _| {
        view.browser.find_only(&THREE);
        let base = rig.fs.location("home");
        view.browser
            .start_find(rig.home, FindQuery::parse("x"), base);
    });
    settle(cx, &rig.view);
    rig.second_tab(cx, rig.dest, "home/deep");
    let before = footer(&rig, cx);
    assert!(before.contains("0 hits in 3 files"), "{before}");
    rig.mark(cx, &[1]);
    let to = rig.list_spot(cx, rig.dest);
    rig.drop_on(cx, (1, to), Modifiers::default());
    settle(cx, &rig.view);
    let after = footer(&rig, cx);
    assert!(after.contains("0 hits in 2 files"), "{after}");
    assert_ne!(before, after);
}
