use std::sync::Arc;

use gpui_kit::TestAppContext;
use rusty_fs::FakeFs;

use super::ReadDone;
use crate::window_view::preview::tests::open_fake;

#[gpui_kit::test]
fn a_late_read_from_an_earlier_expand_does_not_attach(cx: &mut TestAppContext) {
    let fs = Arc::new(FakeFs::new());
    fs.add_file("home/proj/src/main.rs", 3);
    fs.add_file("home/proj/readme.md", 4);
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, |view| {
        view.preview
            .folder()
            .is_some_and(|folder| folder.contents.is_read())
    }));
    let (stale, latest) = rig.view.update(cx, |view, cx| {
        let first = view.preview.ids + 1;
        view.toggle_contents(0, cx);
        view.toggle_contents(0, cx);
        view.toggle_contents(0, cx);
        let folder = view.preview.folder().expect("a folder preview");
        let (load, node) = (folder.load, folder.contents.node_at(0).expect("row 0"));
        let stale = view.folder_read(ReadDone {
            load,
            read: first,
            node,
            listed: Err("stale".to_owned()),
        });
        let latest = view.folder_read(ReadDone {
            load,
            read: first + 1,
            node,
            listed: Ok((Vec::new(), 0)),
        });
        (stale, latest)
    });
    assert!(!stale);
    assert!(latest);
}
