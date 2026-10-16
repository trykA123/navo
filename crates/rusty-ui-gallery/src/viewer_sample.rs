use std::sync::Arc;
use std::sync::atomic::Ordering;

use rusty_core::marks::{Bookmarks, MarkSet};
use rusty_core::paging::{ByteSource, LineIndex, Needle};
use rusty_core::{Location, ProviderId};
use rusty_ui::prelude::*;
use rusty_ui::window_view::{FileHooks, FileStamp, MemoryLink, Paged, PagedKind, PagedView};

const HEX_LEN: usize = 300 * 1024;
const LOG_LINES: usize = 6000;
const INDEX_CHUNK: usize = 64 * 1024;

struct Sample(Arc<Vec<u8>>);

impl ByteSource for Sample {
    fn len(&self) -> u64 {
        self.0.as_slice().len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_slice().read_at(offset, buf)
    }
}

fn mixed_bytes() -> Vec<u8> {
    let mut bytes = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xFF\xFF\x00\x00".to_vec();
    bytes.extend_from_slice(&[0u8; 48]);
    bytes.extend_from_slice(b"This program cannot be run in DOS mode.\r\r\n$\0\0\0\0\0\0\0");
    let mut state: u32 = 0x2545_F491;
    while bytes.len() < HEX_LEN {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let [low, high, ..] = state.to_le_bytes();
        match high % 8 {
            0 => bytes.extend_from_slice(b".text\0\0\0kernel32.dll\0"),
            1 | 2 => bytes.extend_from_slice(&[0; 12]),
            _ => bytes.push(low),
        }
    }
    bytes.truncate(HEX_LEN);
    bytes
}

fn log_lines() -> Vec<u8> {
    let levels = ["INFO", "INFO", "DEBUG", "WARN", "INFO", "ERROR"];
    (0..LOG_LINES)
        .map(|line| {
            let level = levels[line % levels.len()];
            let (minute, second) = (line / 60 % 60, line % 60);
            let detail = if line % 9 == 4 {
                " request=GET /api/library/items?sort=added&order=desc&limit=200&offset=4800 \
                 user=admin agent=ferrite/0.0.1 elapsed_ms=41 cache=miss upstream=10.0.0.12:8096"
            } else {
                ""
            };
            format!(
                "2026-10-15 09:{minute:02}:{second:02} {level:<5} sync: batch {line} done{detail}\n"
            )
        })
        .collect::<String>()
        .into_bytes()
}

fn paged(bytes: Vec<u8>, summary: Option<&str>) -> Paged {
    let bytes = Arc::new(bytes);
    let (reopened, indexed) = (bytes.clone(), bytes.clone());
    Paged {
        source: Arc::new(Sample(bytes)),
        reopen: Arc::new(move || {
            Ok(Box::new(Sample(reopened.clone())) as Box<dyn ByteSource + Send>)
        }),
        index: Arc::new(move |cancel, progress| {
            let mut index = LineIndex::new();
            for chunk in indexed.chunks(INDEX_CHUNK) {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                index.feed(chunk);
                progress(&index);
            }
            index.finish();
            Ok(Some(index))
        }),
        summary: summary.map(str::to_owned),
        editable: false,
        stamp: None,
    }
}

pub fn hex_view(cx: &mut App) -> Entity<PagedView> {
    let sample = paged(mixed_bytes(), Some("PE · 64-bit · x86-64 · program"));
    cx.new(|cx| PagedView::new("ferrite-setup.exe", sample, PagedKind::Hex, cx))
}

pub fn text_view(cx: &mut App) -> Entity<PagedView> {
    let sample = paged(log_lines(), None);
    cx.new(|cx| PagedView::new("sync-2026-10-15.log", sample, PagedKind::Text, cx))
}

fn word(text: &str) -> Needle {
    Needle::Text {
        text: text.to_owned(),
        match_case: true,
    }
}

fn sample_memory() -> (MarkSet, Bookmarks) {
    let mut marks = MarkSet::default();
    for (text, colour) in [("ERROR", 0), ("WARN", 2), ("user=admin", 5)] {
        let _ = marks.set(word(text), colour);
    }
    let mut bookmarks = Bookmarks::default();
    bookmarks.toggle(0);
    (marks, bookmarks)
}

pub fn marks_view(window: &mut Window, cx: &mut App) -> Entity<PagedView> {
    let bytes = log_lines();
    let stamp = FileStamp {
        len: bytes.len() as u64,
        modified_ms: 1,
    };
    let link = MemoryLink {
        hooks: FileHooks {
            memory: Arc::new(|_, _| Some(sample_memory())),
            ..FileHooks::none()
        },
        location: Location::provider_root(ProviderId::local()),
        stamp: Some(stamp),
        carried: None,
        crlf: None,
    };
    let sample = paged(bytes, None);
    cx.new(|cx| {
        let mut view = PagedView::new("sync-2026-10-15.log", sample, PagedKind::Text, cx);
        view.attach_memory(link, cx);
        view.find_all("cache=miss", window, cx);
        view
    })
}
