use std::sync::{Arc, mpsc};
use std::time::Duration;

use gpui_kit::TestAppContext;
use rusty_core::ProviderPath;
use rusty_core::find::{FileHits, LineHit};
use rusty_fs::archive::{ArchiveEntry, ArchiveFormat, ArchiveListing};
use rusty_fs::find::{FindProgress, FoundBatch, FoundFile};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use super::{Log, Setup, editor_of, hooks, redraw, window_with};
use crate::command_center::ContentSource;
use crate::window_view::tests::{fake, press, settle};
use crate::window_view::viewer::archive::ArchiveBody;
use crate::window_view::viewer::{FileHooks, Kinds};
use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;

fn found(relative: &str) -> FoundBatch {
    FoundBatch {
        files: vec![FoundFile {
            relative: ProviderPath::from_text(relative),
            entry: FsEntry {
                name: ProviderPath::from_text("a.txt"),
                kind: EntryKind::File,
                size: 24,
                modified_ms: None,
                attributes: Attributes::default(),
            },
            hits: FileHits {
                hits: vec![LineHit {
                    line: 3,
                    column: 0,
                    text: "needle".into(),
                }],
                total: 1,
                skipped: None,
            },
        }],
        progress: FindProgress::default(),
    }
}

fn preview(
    cx: &mut TestAppContext,
    rig: (
        gpui_kit::AnyWindowHandle,
        &gpui_kit::Entity<crate::window_view::FerriteWindow>,
    ),
    relative: String,
) {
    let (window, view) = rig;
    let (send, receive) = mpsc::channel();
    view.update(cx, |view, _| {
        view.command_center.content_source =
            ContentSource::with_walk(Arc::new(move |_, _, _, sink| {
                let _ = sink(found(&relative));
                send.send(()).unwrap();
                Ok(FindProgress::default())
            }));
    });
    press(cx, window, view, "alt-f7");
    cx.update_window(window, |_, window, cx| window.input("needle arc:on", cx))
        .expect("window");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    receive
        .recv_timeout(Duration::from_secs(2))
        .expect("preview batch");
    cx.executor().advance_clock(Duration::from_millis(16));
    settle(cx, view);
}

#[gpui_kit::test]
fn command_center_enter_opens_the_viewer_at_the_hit_line(cx: &mut TestAppContext) {
    let files = hooks(
        Setup::new(b"first\nsecond\nneedle\nfourth\n".to_vec()),
        &Log::default(),
    );
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    preview(cx, (window, &view), "a.txt".to_owned());
    press(cx, window, &view, "enter");
    for _ in 0..50 {
        settle(cx, &view);
        if editor_of(cx, &view).is_some() {
            break;
        }
    }
    let editor = editor_of(cx, &view).expect("text viewer");
    assert_eq!(
        editor.read_with(cx, |state, _| state.cursor_position().line),
        2
    );
}

fn listing() -> ArchiveListing {
    ArchiveListing {
        format: ArchiveFormat::Zip,
        entries: ["first.txt", "docs/a.txt"]
            .into_iter()
            .map(|name| ArchiveEntry {
                name: name.to_owned(),
                size: 24,
                packed: None,
                modified: None,
                is_folder: false,
                is_encrypted: false,
            })
            .collect(),
        truncated: false,
    }
}

#[gpui_kit::test]
fn command_center_archive_enter_selects_the_matching_entry_or_the_first_row(
    cx: &mut TestAppContext,
) {
    let tree = tempfile::tempdir().expect("tree");
    let archive = tree.path().join("bundle.zip");
    std::fs::write(&archive, b"PK\x03\x04\x14\x00\x00\x00\x08\x00").expect("archive");
    let files = FileHooks {
        archive: Arc::new(|_, _| Ok(listing())),
        ..hooks(
            Setup::new(b"PK\x03\x04\x14\x00\x00\x00\x08\x00".to_vec()),
            &Log::default(),
        )
    };
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    for (entry, expected) in [("docs\\a.txt", "docs/a.txt"), ("missing.txt", "first.txt")] {
        preview(
            cx,
            (window, &view),
            format!("{}\\{entry}", archive.display()),
        );
        press(cx, window, &view, "enter");
        settle(cx, &view);
        redraw(cx, window);
        let selected = view.read_with(cx, |view, cx| {
            let body = view
                .viewer
                .as_ref()?
                .body
                .as_ref()?
                .as_any()
                .downcast_ref::<ArchiveBody>()?;
            Some(body.0.read(cx).selected_entry().to_owned())
        });
        assert_eq!(selected.as_deref(), Some(expected));
        press(cx, window, &view, "escape");
    }
}
