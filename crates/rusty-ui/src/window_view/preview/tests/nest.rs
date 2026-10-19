use gpui_kit::AppContext as _;
use std::sync::Arc;

use gpui_kit::TestAppContext;
use rusty_fs::FakeFs;

use super::{Rig, open_fake};
use crate::window_view::FerriteWindow;
use crate::window_view::preview::Shown;
use crate::window_view::preview::contents::RowKind;
use crate::window_view::preview::spot::Spot;

fn project() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("home/proj/src/deep/note.txt", 70);
    fs.add_file("home/proj/src/main.rs", 300);
    fs.add_file("home/proj/src/lib.rs", 200);
    fs.add_file("home/proj/target/app.exe", 5_000);
    fs.add_file("home/proj/readme.md", 40);
    fs.add_file("home/zz.txt", 1);
    fs
}

fn is_done(view: &FerriteWindow) -> bool {
    view.preview
        .folder()
        .is_some_and(|folder| folder.contents.is_read() && !folder.scan.is_counting())
}

fn shows_folder(name: &str) -> impl Fn(&FerriteWindow) -> bool + '_ {
    move |view| {
        is_done(view)
            && view
                .preview
                .folder()
                .is_some_and(|f| f.target.name() == name)
    }
}

fn browsing(cx: &mut TestAppContext) -> (Rig, Arc<FakeFs>) {
    let fs = Arc::new(project());
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    rig.key(cx, "ctrl-shift-q");
    (rig, fs)
}

fn spot(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    rig.read(cx, |view| {
        let folder = view.preview.folder()?;
        match folder.spot? {
            Spot::Largest(rank) => Some(format!("largest {rank}")),
            Spot::Tree(_) => match folder.contents.row(folder.spot_row()?)?.0 {
                RowKind::Entry(entry) => Some(entry.name.to_text()),
                RowKind::More(_) => None,
            },
        }
    })
}

fn seen(name: &str) -> Option<String> {
    Some(name.to_owned())
}

fn location(cx: &mut TestAppContext, rig: &Rig) -> String {
    rig.read(cx, |view| {
        view.browser
            .state()
            .active_tab()
            .location()
            .as_str()
            .to_owned()
    })
}

fn cursor_name(view: &FerriteWindow) -> Option<String> {
    let tab = view.browser.state().active_tab();
    let row = tab.view().cursor;
    tab.listing()?.get(row).map(|entry| entry.name.clone())
}

fn sheet_has_focus(cx: &mut TestAppContext, rig: &Rig) -> bool {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.read(cx).preview.is_browsing(window, cx)
    })
    .expect("window")
}

fn tree_len(cx: &mut TestAppContext, rig: &Rig) -> usize {
    rig.read(cx, |view| {
        view.preview.folder().map_or(0, |f| f.contents.len())
    })
}

fn shown_file(view: &FerriteWindow) -> Option<String> {
    match view.preview.shown()? {
        Shown::File(file) => Some(file.target.name()),
        _ => None,
    }
}

#[gpui_kit::test]
fn enter_on_a_folder_opens_it_inside_the_preview_and_the_pane_stays(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, shows_folder("src")));
    assert_eq!(location(cx, &rig), fs.location("home").as_str());
    assert!(sheet_has_focus(cx, &rig));
    assert_eq!(spot(cx, &rig), seen("deep"));
    assert_eq!(
        rig.label(cx, "preview-breadcrumb").as_deref(),
        Some("proj › src")
    );
}

#[gpui_kit::test]
fn the_breadcrumb_grows_with_each_folder_and_is_hidden_at_the_start(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    assert!(!rig.shows(cx, "preview-breadcrumb"));
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, shows_folder("src")));
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, shows_folder("deep")));
    assert_eq!(
        rig.label(cx, "preview-breadcrumb").as_deref(),
        Some("proj › src › deep")
    );
    assert_eq!(rig.label(cx, "preview-name").as_deref(), Some("deep"));
    let hints: Vec<String> = (0..6usize)
        .filter_map(|index| rig.label(cx, ("side-sheet-hint", index)))
        .collect();
    assert_eq!(hints, ["Open", "Go to", "Mark", "Up", "Close"]);
}

#[gpui_kit::test]
fn backspace_goes_up_with_the_spot_on_the_folder_it_left(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    rig.key(cx, "down");
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, shows_folder("target")));
    rig.key(cx, "backspace");
    assert!(rig.wait_until(cx, shows_folder("proj")));
    assert_eq!(spot(cx, &rig), seen("target"));
    assert!(!rig.shows(cx, "preview-breadcrumb"));
    assert!(sheet_has_focus(cx, &rig));
    assert_eq!(location(cx, &rig), fs.location("home").as_str());
}

#[gpui_kit::test]
fn backspace_on_the_folder_the_preview_opened_on_stays_there(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    let loads = rig.loads(cx);
    rig.key(cx, "backspace");
    assert_eq!(rig.loads(cx), loads);
    assert_eq!(rig.shown_name(cx).as_deref(), Some("proj"));
    assert_eq!(location(cx, &rig), fs.location("home").as_str());
}

#[gpui_kit::test]
fn shift_enter_on_a_folder_opens_it_in_the_pane_and_not_in_the_preview(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    rig.key(cx, "shift-enter");
    assert_eq!(location(cx, &rig), fs.location("home/proj/src").as_str());
    assert!(rig.list_has_focus(cx));
    assert!(!rig.shows(cx, "preview-breadcrumb"));
}

#[gpui_kit::test]
fn enter_on_a_file_opens_it_read_only_in_the_preview(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    rig.key(cx, "down");
    rig.key(cx, "down");
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, |view| shown_file(view).as_deref() == Some("readme.md")));
    assert_eq!(location(cx, &rig), fs.location("home").as_str());
    assert!(sheet_has_focus(cx, &rig));
    assert_eq!(
        rig.label(cx, "preview-breadcrumb").as_deref(),
        Some("proj › readme.md")
    );
    rig.key(cx, "backspace");
    assert!(rig.wait_until(cx, shows_folder("proj")));
    assert_eq!(spot(cx, &rig), seen("readme.md"));
}

#[gpui_kit::test]
fn enter_on_a_largest_file_opens_it_in_the_preview(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    for _ in 0..3 {
        rig.key(cx, "down");
    }
    assert_eq!(spot(cx, &rig), seen("largest 0"));
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, |view| shown_file(view).as_deref() == Some("app.exe")));
    assert_eq!(location(cx, &rig), fs.location("home").as_str());
}

#[gpui_kit::test]
fn shift_enter_on_a_file_reveals_it_in_the_pane(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    rig.key(cx, "down");
    rig.key(cx, "down");
    rig.key(cx, "shift-enter");
    let folder = fs.location("home/proj");
    assert!(rig.wait_until(cx, |view| {
        view.browser.state().active_tab().location() == &folder
            && cursor_name(view).as_deref() == Some("readme.md")
    }));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn reopening_the_preview_starts_again_at_the_cursor(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "enter");
    assert!(rig.wait_until(cx, shows_folder("src")));
    rig.key(cx, "escape");
    rig.open(cx);
    assert!(rig.wait_until(cx, shows_folder("proj")));
    assert!(!rig.shows(cx, "preview-breadcrumb"));
}

#[gpui_kit::test]
fn alt_right_unfolds_every_folder_below_and_alt_left_folds_them_again(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "alt-right");
    assert!(rig.wait_until(cx, |view| {
        view.preview.folder().is_some_and(|f| f.contents.len() == 7)
    }));
    assert_eq!(spot(cx, &rig), seen("src"));
    rig.key(cx, "alt-left");
    assert_eq!(tree_len(cx, &rig), 3);
    assert_eq!(spot(cx, &rig), seen("src"));
    rig.key(cx, "right");
    assert!(rig.wait_until(cx, |view| {
        view.preview.folder().is_some_and(|f| f.contents.len() == 6)
    }));
}

#[gpui_kit::test]
fn right_and_left_still_open_and_close_one_folder_in_place(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "right");
    assert!(rig.wait_until(cx, |view| {
        view.preview.folder().is_some_and(|f| f.contents.len() == 6)
    }));
    assert_eq!(rig.shown_name(cx).as_deref(), Some("proj"));
    rig.key(cx, "left");
    assert_eq!(tree_len(cx, &rig), 3);
}
