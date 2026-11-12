//! The file diff's line widths (D39.4, PC4): every line's columns are measured off the UI thread,
//! once per opened text, and give the sideways state its widest line when they land.

use std::sync::Arc;

use gpui_kit::Task;

use crate::compare_diff::{LineCols, PairText};
use crate::prelude::*;
use crate::window_view::ScreenOwner;

/// The width measure of one opened text: whether it started, and its running task.
#[derive(Default)]
pub(super) struct WidthsSlot {
    started: bool,
    /// The wait for the background measure; dropping it drops the result.
    task: Option<Task<()>>,
}

impl ScreenOwner {
    /// Starts measuring the open text's line widths on a background thread, once per opened
    /// text. Until they land the sideways extent is unknown, so nothing is cut.
    pub(in crate::window_view::compare_screen) fn start_file_widths(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let Some(view) = file
            .and_then(|file| file.view.as_mut())
            .filter(|view| !view.widths.started)
        else {
            return;
        };
        view.widths.started = true;
        let text = Arc::clone(view.text());
        let measured = Arc::clone(&text);
        let measuring = cx.background_spawn(async move { LineCols::measure(&measured) });
        view.widths.task = Some(cx.spawn(async move |this, cx| {
            let cols = measuring.await;
            let _ = this.update(cx, |window, cx| window.install_file_widths(&text, cols, cx));
        }));
    }

    /// Keeps `cols`, measured from `text`, on the open view if it still shows that same text
    /// (`Arc::ptr_eq`), and redraws; a result for a replaced or closed text is dropped.
    pub(in crate::window_view) fn install_file_widths(
        &mut self,
        text: &Arc<PairText>,
        cols: LineCols,
        cx: &mut Context<Self>,
    ) {
        let file = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let view = file
            .and_then(|file| file.view.as_mut())
            .filter(|view| Arc::ptr_eq(view.text(), text));
        if let Some(view) = view {
            let cols = Arc::new(cols);
            view.sideways.set_cols(&cols);
            view.cols = Some(cols);
            view.widths.task = None;
            cx.notify();
        }
    }
}
