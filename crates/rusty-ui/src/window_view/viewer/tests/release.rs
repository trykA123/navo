use std::time::Duration;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_fs::FakeFs;

use super::{redraw, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::held::{
    Disk, assert_released, cursor_to, gpu_images, held_in, samples,
};
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::map::tests::SAMPLE as MAP;
use crate::window_view::viewer::mdf::tests::DEMO as MDF;
use crate::window_view::viewer::{Kinds, VIEWER_DOCUMENTS};

const DBC: &[u8] = b"BO_ 42 Frame: 8 Sender\n SG_ Speed : 0|8@1+ (1,0) [0|255] \"rpm\" Receiver\n";
const A2L: &[u8] = br#"ASAP2_VERSION 1 71
/begin PROJECT Invented ""
/begin MODULE Demo ""
/end MODULE
/end PROJECT
"#;
const ASC: &[u8] = b"date Mon Oct 27 10:00:00.000 am 2026\nbase hex  timestamps absolute\n   0.010000 1  123             Rx   d 2 01 02\n";
const INTEL_HEX: &[u8] = b":0400000001020304F2\n:00000001FF\n";

type Rig = (AnyWindowHandle, Entity<FerriteWindow>, Disk);

fn every_kind() -> Vec<(&'static str, Vec<u8>)> {
    let elf = rusty_automotive::elf_sample::demo().expect("the demo builds");
    let mut files = samples();
    files.extend([
        ("app.elf", elf),
        ("app.hex", INTEL_HEX.to_vec()),
        ("app.map", MAP.as_bytes().to_vec()),
        ("bus.asc", ASC.to_vec()),
        ("chips.mdf", MDF.as_bytes().to_vec()),
        ("sample.a2l", A2L.to_vec()),
        ("sample.dbc", DBC.to_vec()),
    ]);
    files
}

fn rig(cx: &mut TestAppContext, files: &[(&str, Vec<u8>)]) -> Rig {
    let fs = FakeFs::new();
    fs.add_folder("dest");
    for (name, bytes) in files {
        fs.add_file(&format!("home/{name}"), bytes.len() as u64);
    }
    let disk = Disk::new(files);
    let (window, view) = window_with(cx, std::sync::Arc::new(fs), disk.hooks(), Kinds::builtin());
    (window, view, disk)
}

fn rest(cx: &mut TestAppContext, (window, view, _): &Rig) {
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(cx, view);
        redraw(cx, *window);
    }
}

fn body(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<gpui_kit::AnyWeakView> {
    view.read_with(cx, |view, _| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        Some(body.view().downgrade())
    })
}

fn kind(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> &'static str {
    view.read_with(cx, |view, _| {
        let kind = view.viewer.as_ref()?.kind.as_ref()?;
        Some(kind.id())
    })
    .unwrap_or_default()
}

fn open(cx: &mut TestAppContext, rig: &Rig, name: &str) -> gpui_kit::AnyWeakView {
    cursor_to(cx, (rig.0, &rig.1), name);
    press(cx, rig.0, &rig.1, "f3");
    rest(cx, rig);
    body(cx, &rig.1).unwrap_or_else(|| panic!("{name}: no body"))
}

fn close(cx: &mut TestAppContext, rig: &Rig) {
    press(cx, rig.0, &rig.1, "escape");
    rest(cx, rig);
    assert!(body(cx, &rig.1).is_none());
}

#[gpui_kit::test]
fn every_kind_closed_in_the_viewer_holds_no_view_file_or_gpu_image(cx: &mut TestAppContext) {
    let files = every_kind();
    let rig = rig(cx, &files);
    let mut shown_kinds = Vec::new();
    for (name, _) in &files {
        let weak = open(cx, &rig, name);
        assert_eq!(held_in(cx, &VIEWER_DOCUMENTS).items, 1, "{name}");
        shown_kinds.push(kind(cx, &rig.1));
        close(cx, &rig);
        assert_eq!(held_in(cx, &VIEWER_DOCUMENTS).items, 0, "{name}");
        assert_released(cx, name, (&weak, &rig.2));
    }
    let expected = [
        "hex",
        "archive",
        "tree",
        "font",
        "tree",
        "pdf",
        "text",
        "image",
        "markdown",
        "table",
        "tree",
        "image",
        "elf",
        "memory",
        "linker-map",
        "can-trace",
        "memory-definition",
        "a2l",
        "dbc",
    ];
    assert_eq!(shown_kinds, expected);
}

#[gpui_kit::test]
fn pictures_and_pdf_pages_are_uploaded_while_open(cx: &mut TestAppContext) {
    let files = samples();
    let rig = rig(cx, &files);
    for name in ["manual.pdf", "photo.png", "sign.svg"] {
        let weak = open(cx, &rig, name);
        assert!(gpu_images(cx).items > 0, "{name}");
        close(cx, &rig);
        assert_released(cx, name, (&weak, &rig.2));
    }
}

#[gpui_kit::test]
fn pdf_pages_redrawn_on_zoom_and_page_moves_are_all_released(cx: &mut TestAppContext) {
    let files = samples();
    let rig = rig(cx, &files);
    let weak = open(cx, &rig, "manual.pdf");
    assert_eq!(rig.2.open_handles(), 1, "the PDF helper holds the file");
    let first = gpu_images(cx).items;
    for key in ["ctrl-1", "ctrl-=", "ctrl-=", "end", "home", "ctrl-0"] {
        press(cx, rig.0, &rig.1, key);
        rest(cx, &rig);
    }
    assert!(
        gpu_images(cx).items <= first + 2,
        "replaced pages are released"
    );
    close(cx, &rig);
    assert_released(cx, "manual.pdf", (&weak, &rig.2));
}

#[gpui_kit::test]
fn moving_to_the_next_file_releases_the_previous_view(cx: &mut TestAppContext) {
    let files = samples();
    let rig = rig(cx, &files);
    let weak = open(cx, &rig, "photo.png");
    press(cx, rig.0, &rig.1, "space");
    rest(cx, &rig);
    assert!(weak.upgrade().is_none());
    let next = body(cx, &rig.1).expect("the next file is open");
    close(cx, &rig);
    assert_released(cx, "next file", (&next, &rig.2));
}
