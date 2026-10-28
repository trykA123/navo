use rusty_core::find::FindQuery;
use rusty_core::{Command, Location, PaneId, Tab};
use rusty_l10n::t;

use super::FerriteWindow;
use crate::pane_chrome::PathBar;
use crate::prelude::*;
use crate::settings::WindowParts;

mod filters;

pub(super) fn path_text(query: &FindQuery) -> SharedString {
    let filters = filters::text(query);
    if filters.is_empty() {
        t!("find-path", text = query.text.clone()).into()
    } else {
        t!(
            "find-path-filtered",
            text = query.text.clone(),
            filters = filters
        )
        .into()
    }
}

impl FerriteWindow {
    pub(super) fn search_tabs(&self, pane: PaneId) -> Vec<usize> {
        self.browser
            .state()
            .pane(pane)
            .map(|pane| {
                pane.tabs()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, tab)| self.browser.find_results(tab.id()).map(|_| index))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn tab_title(&self, tab: &Tab) -> SharedString {
        match self.browser.find_results(tab.id()) {
            Some(results) if results.query.text.is_empty() => t!("find-title-empty").into(),
            Some(results) => results.query.text.clone().into(),
            None => super::pane::title_of(tab),
        }
    }

    pub(super) fn find_status(&self, pane: PaneId) -> Option<SharedString> {
        let tab = self.browser.state().pane(pane)?.active();
        let results = self.browser.find_results(tab.id())?;
        let hits = results.progress.hits.to_string();
        let files = results.progress.matched.to_string();
        let counts = t!("find-counts", hits = hits, files = files);
        Some(if results.running() {
            t!("find-searching", counts = counts).into()
        } else {
            counts.into()
        })
    }

    pub(super) fn stop_find(&mut self) -> bool {
        let tab = self.browser.state().active_tab().id();
        if !self
            .browser
            .find_results(tab)
            .is_some_and(crate::browser::FindResults::running)
        {
            return false;
        }
        self.browser.stop_find(tab);
        true
    }

    pub(super) fn path_row(
        &self,
        pane: PaneId,
        parts: WindowParts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(input) = self.path_input(pane, cx) {
            return Some(input.into_any_element());
        }
        if !parts.path_bar {
            return None;
        }
        let state = self.browser.state();
        let location = state
            .pane(pane)
            .map(|p| p.active().location().clone())
            .unwrap_or_else(|| state.active_tab().location().clone());
        let tab = state
            .pane(pane)
            .map_or_else(|| state.active_tab(), |pane| pane.active());
        if let Some(results) = self.browser.find_results(tab.id()) {
            let bar = PathBar::new(("path", pane.get()), location)
                .search(path_text(&results.query), results.running());
            return Some(bar.into_any_element());
        }
        let menu = self.chrome.get(pane).map(|chrome| chrome.path.clone());
        let bar = Self::path_bar(pane, location, cx).when_some(menu, |bar, menu| bar.menu(menu));
        Some(bar.into_any_element())
    }

    fn path_bar(pane: PaneId, location: Location, cx: &mut Context<Self>) -> PathBar {
        let edit = cx.weak_entity();
        let bar = PathBar::new(("path", pane.get()), location);
        Self::wire_crumb_drops(bar, pane, cx)
            .on_navigate(
                cx.listener(move |this: &mut Self, location: &Location, window, cx| {
                    let command = Command::PaneNavigate {
                        location: location.clone(),
                    };
                    this.focus_pane(pane);
                    this.apply(&command, window, cx);
                }),
            )
            .on_edit(move |_, cx| {
                let _ = edit.update(cx, |view, cx| {
                    view.focus_pane(pane);
                    view.open_path_edit(pane, cx);
                });
            })
    }
}
