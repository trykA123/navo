//! What the pane's folder list knows about its listing: finished or not, and how many entries it holds.

use navo_core::PaneId;

use super::NavoWindow;

impl NavoWindow {
    /// The pane's list state: (listing finished, entries before any filter).
    pub(super) fn pane_listing(&self, pane: PaneId) -> (bool, u64) {
        (self.listing_finished(pane), self.listed_rows(pane))
    }

    /// True once the pane's tab has a listing, no error and no load under way, so zero rows mean an empty folder.
    fn listing_finished(&self, pane: PaneId) -> bool {
        let state = self.browser.state();
        state
            .pane(pane)
            .map(|core| core.active())
            .is_some_and(|tab| {
                tab.listing().is_some()
                    && tab.pending_request().is_none()
                    && self.waited(pane, tab).is_none()
                    && self.pane_error(pane, tab).is_none()
            })
    }

    /// The entry count of the pane's folder before any filter.
    fn listed_rows(&self, pane: PaneId) -> u64 {
        let state = self.browser.state();
        state
            .pane(pane)
            .and_then(|core| core.active().listing())
            .map_or(0, |listing| listing.len())
    }
}
