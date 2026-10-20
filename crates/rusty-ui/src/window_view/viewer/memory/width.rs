use gpui_kit::{TextRun, Window, font};
use rusty_automotive::Row;

use super::jobs::options;
use super::{Index, MemoryView, Ready};
use crate::prelude::*;
use crate::tokens::Theme;

const NARROW: u64 = 16;
const WIDE: u64 = 32;
const FALLBACK_CHAR_WIDTH: f32 = 7.2;
const SAMPLE: &str = "0000000000";
const SCROLLBAR: f32 = 16.;

pub(super) fn row_chars(row_bytes: u64, address_chars: usize) -> usize {
    let n = row_bytes as usize;
    let hex = n * 3 - 1 + n / 8 - 1;
    address_chars + hex + n
}

pub(super) fn pick_row_bytes(
    available: f32,
    char_width: f32,
    address_chars: usize,
    chrome: f32,
) -> u64 {
    let needed = row_chars(WIDE, address_chars) as f32 * char_width + chrome + SCROLLBAR;
    if available >= needed { WIDE } else { NARROW }
}

fn measure_char_width(window: &Window, t: &Theme) -> f32 {
    let run = TextRun {
        len: SAMPLE.len(),
        font: font(t.typography.mono_font.clone()),
        color: t.colors.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .layout_line(SAMPLE, t.typography.body, &[run], None);
    let width = f32::from(line.width) / SAMPLE.len() as f32;
    if width > 0. && width.is_finite() {
        width
    } else {
        FALLBACK_CHAR_WIDTH
    }
}

impl MemoryView {
    pub(super) fn wanted_row_bytes(&self, window: &Window, t: &Theme) -> u64 {
        let wide = self.ready().is_some_and(|ready| ready.wide);
        let chars = if wide { 18 } else { 10 };
        let chrome = f32::from(t.spacing.xl) * 4.;
        pick_row_bytes(
            f32::from(window.viewport_size().width),
            measure_char_width(window, t),
            chars,
            chrome,
        )
    }

    pub(super) fn fit_row_bytes(
        &mut self,
        wanted: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ready().is_none() {
            self.row_bytes = wanted;
            return;
        }
        if self
            .ready()
            .is_some_and(|ready| ready.layout.row_bytes() == wanted)
        {
            return;
        }
        cx.defer_in(window, move |this, _, cx| this.set_row_bytes(wanted, cx));
    }

    pub(super) fn set_row_bytes(&mut self, wanted: u64, cx: &mut Context<Self>) {
        let cursor = self.cursor;
        let Index::Ready(ready) = &mut self.index else {
            return;
        };
        if ready.layout.row_bytes() == wanted {
            return;
        }
        let at = cursor_address(ready, cursor);
        ready.layout = rusty_automotive::Layout::new(&ready.image.ranges, options(wanted));
        let row = at.and_then(|at| ready.layout.row_of(at)).unwrap_or(0);
        self.row_bytes = wanted;
        self.fetched = None;
        self.fetching = None;
        self.cursor = row;
        self.scroll
            .scroll_to(usize::try_from(row).unwrap_or(usize::MAX));
        cx.notify();
    }
}

fn cursor_address(ready: &Ready, cursor: u64) -> Option<u64> {
    match ready.layout.row(cursor)? {
        Row::Data { address, .. } => Some(address),
        Row::Gap { range } => Some(range.start),
    }
}
