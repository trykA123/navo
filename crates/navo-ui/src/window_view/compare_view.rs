//! Compare mode on screen: each compared pane draws its side's rows, one bar sits above both.

use std::sync::Arc;

use navo_core::PaneId;

use super::NavoWindow;
use crate::FocusHandle;
use crate::browser::{RowView, bar_view, empty_text, pane_rows};
use crate::compare_list::{BarEvent, CompareBar, CompareEvent, CompareList};
use crate::prelude::*;
use crate::primitives::ListScroll;

/// The rows built for one run and revision, so a frame does not rebuild 66,000 of them.
struct Built {
    key: (u64, u64),
    sides: [Arc<[RowView]>; 2],
}

#[derive(Default)]
pub(super) struct CompareView {
    scroll: ListScroll,
    built: Option<Built>,
    scrolled_to: Option<String>,
}

impl NavoWindow {
    /// The compare list for `pane`, or `None` when the pane is not part of the compare.
    pub(super) fn compare_list(
        &mut self,
        pane: PaneId,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mode = self.browser.compare_mode()?;
        let side = mode.side_of(pane)?;
        let key = (mode.run(), mode.revision());
        if self.compare.built.as_ref().map(|built| built.key) != Some(key) {
            self.compare.built = Some(Built {
                key,
                sides: [0, 1].map(|side| pane_rows(mode, side)),
            });
        }
        let built = self.compare.built.as_ref()?;
        let rows = Arc::clone(&built.sides[side]);
        let selected = mode.selected_path();
        let at = selected.and_then(|path| rows.iter().position(|row| row.path == path));
        if self.compare.scrolled_to.as_deref() != selected {
            match (selected, at) {
                (None, _) => self.compare.scrolled_to = None,
                (Some(path), Some(at)) => {
                    self.compare.scroll.scroll_to(at);
                    self.compare.scrolled_to = Some(path.to_owned());
                }
                (Some(_), None) => {}
            }
        }
        // The empty text follows totals, which do not move the revision.
        let empty = rows.is_empty().then(|| empty_text(mode)).flatten();
        let label = navo_l10n::tr(["compare-list-left", "compare-list-right"][side]);
        let list = CompareList::new(("compare-list", pane.get()), rows, &self.compare.scroll)
            .selected(at)
            .active(self.browser.state().focused_pane() == pane)
            .empty_text(empty)
            .label(label)
            .track_focus(focus)
            .on_event(
                cx.listener(move |this: &mut Self, event: &CompareEvent, window, cx| {
                    this.compare_event((pane, event), window, cx);
                }),
            );
        Some(list.into_any_element())
    }

    /// The filter row and totals line, or `None` outside compare mode.
    pub(super) fn compare_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let view = bar_view(self.browser.compare_mode()?);
        let bar = CompareBar::new("compare-bar", view).on_event(cx.listener(
            |this: &mut Self, event: &BarEvent, window, cx| this.bar_event(event, window, cx),
        ));
        Some(bar.into_any_element())
    }

    fn compare_event(
        &mut self,
        (pane, event): (PaneId, &CompareEvent),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_pane(pane);
        match event {
            CompareEvent::Select(path) => {
                self.browser.compare_select(path);
            }
            CompareEvent::Open(path) => {
                self.browser.compare_select(path);
                self.browser.compare_enter();
            }
        }
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    pub(super) fn bar_event(
        &mut self,
        event: &BarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            BarEvent::Filter(filter) => {
                self.browser.compare_filter(*filter);
            }
            BarEvent::Leave => self.browser.leave_compare(),
            BarEvent::Continue => {
                self.browser.compare_continue();
            }
        }
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}
