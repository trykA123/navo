//! Reading the two sides of an open binary pair and its visible hex rows on background
//! threads (V39), so the UI never blocks on file I/O.

use std::ops::Range;

use gpui_kit::ScrollEvent;

use super::NavoWindow;
use super::hex;
use crate::compare_diff::PairRequest;
use crate::prelude::*;
use crate::tokens;

impl NavoWindow {
    /// Reads the open binary pair's sides on a background thread and keeps the task on the view.
    pub(super) fn start_hex_open(&mut self, cx: &mut Context<Self>) {
        let Some((tag, active, request)) = self.pending_hex() else {
            return;
        };
        let files = self.files.clone();
        let opening =
            cx.background_spawn(async move { hex::HexView::open_sides(&request, &files) });
        let task = cx.spawn(async move |this, cx| {
            let opened = opening.await;
            let _ = this.update(cx, |view, cx| view.install_hex((tag, active), opened, cx));
        });
        if let Some(hex) = self.open_hex_mut() {
            hex.set_open_task(task);
        }
    }

    /// The tag and pair of a binary body that has not started opening; starts it once.
    fn pending_hex(&mut self) -> Option<(u64, u64, PairRequest)> {
        let file = self.compare.screen.as_mut()?.file.as_mut()?;
        let hex = file.hex.as_mut()?;
        hex.begin_open()
            .then(|| (file.tag, file.active, file.request.clone()))
    }

    /// Installs one background read of both hex sides if `(tag, active)` still match.
    fn install_hex(
        &mut self,
        (tag, active): (u64, u64),
        opened: hex::SidesOpened,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        if file.tag != tag || file.active != active {
            return;
        }
        let Some(hex) = file.hex.as_mut() else {
            return;
        };
        hex.install(opened);
        let requested = hex.request_regions().map(|run| (hex.align(), run));
        if let Some((align, run)) = requested {
            file.control.byte_regions(align, run);
        }
        cx.notify();
    }

    /// Fetches the visible rows' bytes off the UI thread; the body draws the filled state.
    pub(in crate::window_view) fn request_hex_rows(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let t = tokens::current(cx);
        let visible = hex::visible_rows(t, window);
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        let (tag, active) = (file.tag, file.active);
        let Some(hex) = file.hex.as_mut() else {
            return;
        };
        hex.take_drag(visible);
        hex.refresh_scroll(visible);
        let Some((sources, reads, range)) = hex.begin_fetch(hex.viewport(visible)) else {
            return;
        };
        let job = cx.background_spawn(async move { hex::fetch_rows(&sources, &reads) });
        let task = cx.spawn(async move |this, cx| {
            let rows = job.await;
            let _ = this.update(cx, |view, cx| {
                view.install_hex_rows((tag, active), (range, rows), cx);
            });
        });
        if let Some(hex) = self.open_hex_mut() {
            hex.set_fetch_task(task);
        }
    }

    /// Installs one fetched row range if its screen and request are still current.
    fn install_hex_rows(
        &mut self,
        (tag, active): (u64, u64),
        (range, rows): (Range<usize>, hex::Fetched),
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        if file.tag != tag || file.active != active {
            return;
        }
        let Some(hex) = file.hex.as_mut() else {
            return;
        };
        hex.install_rows(range, rows);
        cx.notify();
    }

    /// Wheel scrolling of the hex body.
    pub(super) fn scroll_hex(
        &mut self,
        event: &ScrollEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let t = tokens::current(cx);
        let visible = hex::visible_rows(t, window);
        if let Some(hex) = self.open_hex_mut() {
            hex.wheel(event, visible, t.spacing.row_height);
            hex.refresh_scroll(visible);
            cx.notify();
        }
    }

    /// The open hex body, if the file diff is a binary pair.
    fn open_hex_mut(&mut self) -> Option<&mut hex::HexView> {
        self.compare.screen.as_mut()?.file.as_mut()?.hex.as_mut()
    }
}
