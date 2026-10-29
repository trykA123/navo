use std::rc::Rc;
use std::sync::Arc;

use rusty_core::{ProviderPath, Selection, ViewState};
use rusty_fs::{Attributes, EntryKind, FsEntry};
use rusty_ui::FocusHandle;
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::prelude::*;
use rusty_ui::tokens;

const MODIFIED_MS: i64 = 1_791_920_400_000;

const NAMES: [&str; 30] = [
    "report.pdf",
    "letter.docx",
    "budget.xlsx",
    "talk.pptx",
    "notes.txt",
    "README.md",
    "song.mp3",
    "clip.mp4",
    "inter.woff2",
    "app.ini",
    "server.pem",
    "cache.sqlite",
    "firmware.elf",
    "ecu.a2l",
    "powertrain.dbc",
    "drive.blf",
    "link.map",
    "main.rs",
    "build.sh",
    "package.json",
    "index.html",
    "photo.png",
    "backup.zip",
    "setup.exe",
    "UPPER.PDF",
    "flash.s19",
    "signals.mf4",
    "layout.ldf",
    "style.css",
    "unknown.xyz",
];

pub struct KindsPage {
    rows: Arc<[FsEntry]>,
    focus: FocusHandle,
}

fn file(name: &str) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 48_000,
        modified_ms: Some(MODIFIED_MS),
        attributes: Attributes::default(),
    }
}

impl KindsPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            rows: NAMES.iter().map(|name| file(name)).collect(),
            focus: cx.focus_handle(),
        }
    }
}

impl Render for KindsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let props = FileListProps {
            folder: "file types".to_owned().into(),
            rows: self.rows.clone(),
            view: ViewState {
                cursor: 0,
                scroll: 0,
            },
            marked: Rc::new(Selection::new(self.rows.len() as u64)),
            is_active: true,
            filter: None,
            sort: SortKey::default(),
        };
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface)
            .child(FileList::new("kinds-list", props).track_focus(&self.focus))
    }
}
