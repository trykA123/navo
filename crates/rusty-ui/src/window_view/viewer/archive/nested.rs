use std::sync::Arc;
use std::sync::atomic::Ordering;

use rusty_core::paging::{ByteSource, LineIndex};

use crate::prelude::*;
use crate::window_view::viewer::paged::PagedBody;
use crate::window_view::viewer::{Paged, PagedKind, PagedView, Sniff};

const INDEX_CHUNK: usize = 64 * 1024;
const SNIFF: usize = 8 * 1024;

struct Memory(Arc<Vec<u8>>);

impl ByteSource for Memory {
    fn len(&self) -> u64 {
        self.0.as_slice().len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_slice().read_at(offset, buf)
    }
}

fn paged(bytes: Arc<Vec<u8>>) -> Paged {
    let (reopened, indexed) = (bytes.clone(), bytes.clone());
    Paged {
        source: Arc::new(Memory(bytes)),
        reopen: Arc::new(move || {
            Ok(Box::new(Memory(reopened.clone())) as Box<dyn ByteSource + Send>)
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
        summary: None,
        problem: None,
        editable: false,
        stamp: None,
    }
}

pub(super) struct Entry {
    pub title: SharedString,
    pub name: String,
    pub bytes: Vec<u8>,
}

pub(super) fn open(entry: Entry, cx: &mut App) -> PagedBody {
    let sniff = Sniff {
        head: entry.bytes.get(..SNIFF).unwrap_or(&entry.bytes),
        name: &entry.name,
        len: entry.bytes.len() as u64,
    };
    let kind = if sniff.looks_binary() || rusty_core::encoding::is_utf16(sniff.head) {
        PagedKind::Hex
    } else {
        PagedKind::Text
    };
    let paged = paged(Arc::new(entry.bytes));
    PagedBody(cx.new(|cx| PagedView::new(entry.title, paged, kind, cx)))
}
