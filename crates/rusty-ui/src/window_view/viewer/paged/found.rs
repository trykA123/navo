use std::rc::Rc;

use rusty_core::paging::{
    ByteSource, HEX_WIDTH, LineIndex, hex_row, hex_row_start, read_full, row_at, row_start_at,
};

use super::PagedView;
use super::lines::PagedKind;
use crate::prelude::*;
use crate::window_view::viewer::search::find_all::{FoundRow, PanelRows, preview};

const PREVIEW_CAP: usize = 4096;

struct Probe<'a, S: ?Sized> {
    source: &'a S,
    kind: PagedKind,
    index: Option<&'a LineIndex>,
    width: u64,
}

fn hex_found<S: ByteSource + ?Sized>(probe: &Probe<'_, S>, at: u64) -> Result<FoundRow, String> {
    let start = hex_row_start(at);
    let mut bytes = [0u8; 16];
    let read = read_full(probe.source, start, &mut bytes)?;
    let row = hex_row(
        start,
        &bytes[..read],
        probe.source.len() > u64::from(u32::MAX),
    );
    let first = usize::try_from(at - start).unwrap_or(0);
    let last = usize::try_from((at + probe.width.max(1) - 1).min(start + HEX_WIDTH - 1) - start)
        .unwrap_or(first);
    let column = |slot: usize| slot * 3 + usize::from(slot >= 8);
    Ok(FoundRow {
        place: row.offset.into(),
        text: row.hex.into(),
        hit: column(first)..column(last) + 2,
    })
}

fn text_found<S: ByteSource + ?Sized>(probe: &Probe<'_, S>, at: u64) -> Result<FoundRow, String> {
    let source = probe.source;
    let row = row_at(source, row_start_at(source, at)?)?;
    let mut bytes = vec![0u8; usize::try_from(row.end - row.start).unwrap_or(0)];
    let read = read_full(source, row.start, &mut bytes)?;
    let line = String::from_utf8_lossy(&bytes[..read]);
    let hit = usize::try_from(at - row.start).unwrap_or(0);
    let width = usize::try_from(probe.width).unwrap_or(0);
    let number = match probe.index {
        Some(index) => index.line_number(source, row.start)?,
        None => None,
    };
    let place = number.map_or_else(String::new, |number| number.to_string());
    Ok(preview(place, &line, hit..hit + width))
}

fn found_row<S: ByteSource + ?Sized>(probe: &Probe<'_, S>, at: u64) -> FoundRow {
    let found = match probe.kind {
        PagedKind::Hex => hex_found(probe, at),
        PagedKind::Text => text_found(probe, at),
    };
    found.unwrap_or_else(|error| FoundRow {
        place: SharedString::default(),
        text: error.into(),
        hit: 0..0,
    })
}

impl PagedView {
    pub(super) fn found_rows(&self, cx: &mut Context<Self>) -> PanelRows {
        let reader = cx.entity().downgrade();
        let loader = reader.clone();
        PanelRows {
            source: Rc::new(move |start, cx| {
                reader.upgrade()?.read(cx).previews.get(&start).cloned()
            }),
            on_missing: Some(Rc::new(move |starts, cx| {
                let _ = loader.update(cx, |view, cx| view.load_previews(starts, cx));
            })),
        }
    }

    fn load_previews(&mut self, starts: Vec<u64>, cx: &mut Context<Self>) {
        if self.preview_job.is_some() {
            return;
        }
        let Some(width) = self.marking.find_all_width() else {
            return;
        };
        let (source, kind, index) = (self.source.clone(), self.kind, self.index.clone());
        self.preview_job = Some(cx.spawn(async move |this, cx| {
            let rows = cx
                .background_spawn(async move {
                    let probe = Probe {
                        source: &*source,
                        kind,
                        index: index.as_deref(),
                        width,
                    };
                    let rows: Vec<(u64, FoundRow)> = starts
                        .into_iter()
                        .map(|at| (at, found_row(&probe, at)))
                        .collect();
                    rows
                })
                .await;
            let _ = this.update(cx, |view, cx| view.previews_loaded(rows, cx));
        }));
    }

    fn previews_loaded(&mut self, rows: Vec<(u64, FoundRow)>, cx: &mut Context<Self>) {
        self.preview_job = None;
        if self.previews.len() + rows.len() > PREVIEW_CAP {
            self.previews.clear();
        }
        self.previews.extend(rows);
        cx.notify();
    }
}
