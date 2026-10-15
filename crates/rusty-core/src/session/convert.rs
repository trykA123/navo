use std::collections::HashSet;

use super::{EntryState, PaneState, RowsState, SessionState, TabState, WindowState};
use crate::{
    AppState, History, Layout, Location, Pane, PaneId, Tab, TabId, ViewState, Window, WindowId,
};

pub(super) fn save(state: &AppState, rows_per_pane: usize) -> SessionState {
    let windows = state
        .window_ids()
        .into_iter()
        .filter_map(|id| state.window(id))
        .map(|w| save_window(w, rows_per_pane))
        .collect();
    SessionState {
        windows,
        focused_window: state.focused_window().get(),
        next_id: state.next_id(),
    }
}

fn save_window(window: &Window, rows_per_pane: usize) -> WindowState {
    WindowState {
        id: window.id().get(),
        layout: window.layout().to_state(),
        focused_pane: window.layout().focused().get(),
        panes: window
            .panes()
            .into_iter()
            .map(|pane| PaneState {
                id: pane.id().get(),
                active: pane.active_index(),
                page_rows: pane.page_rows(),
                tabs: pane
                    .tabs()
                    .iter()
                    .map(|t| save_tab(t, rows_per_pane))
                    .collect(),
            })
            .collect(),
    }
}

fn save_tab(tab: &Tab, rows_per_pane: usize) -> TabState {
    let strings = |list: &[Location]| list.iter().map(|l| l.as_str().to_owned()).collect();
    TabState {
        id: tab.id().get(),
        location: tab.location().as_str().to_owned(),
        back: strings(tab.history().back()),
        forward: strings(tab.history().forward()),
        cursor: tab.view().cursor,
        scroll: tab.view().scroll,
        rows: tab.listing().map(|listing| {
            let entries = listing.entries();
            let first = usize::try_from(tab.view().scroll)
                .unwrap_or(usize::MAX)
                .min(entries.len());
            let end = first.saturating_add(rows_per_pane).min(entries.len());
            RowsState {
                first: first as u64,
                total: listing.len(),
                entries: entries[first..end]
                    .iter()
                    .map(|e| EntryState {
                        name: e.name.clone(),
                        is_folder: e.is_folder,
                    })
                    .collect(),
            }
        }),
    }
}

pub(super) fn restore(session: &SessionState) -> Result<AppState, String> {
    if session.windows.is_empty() {
        return Err("the session has no windows".to_owned());
    }
    check_ids(session)?;
    let windows = session
        .windows
        .iter()
        .map(restore_window)
        .collect::<Result<Vec<_>, _>>()?;
    AppState::restore(
        windows,
        WindowId::new(session.focused_window),
        session.next_id,
    )
}

fn check_ids(session: &SessionState) -> Result<(), String> {
    let mut seen = HashSet::new();
    let ids = session.windows.iter().flat_map(|w| {
        std::iter::once(w.id).chain(
            w.panes
                .iter()
                .flat_map(|p| std::iter::once(p.id).chain(p.tabs.iter().map(|t| t.id))),
        )
    });
    for id in ids {
        if !seen.insert(id) {
            return Err(format!("id {id} is used twice"));
        }
        if id >= session.next_id {
            return Err(format!("id {id} is not below next_id {}", session.next_id));
        }
    }
    Ok(())
}

fn restore_window(window: &WindowState) -> Result<Window, String> {
    let layout = Layout::from_state(&window.layout, PaneId::new(window.focused_pane))?;
    let panes = window
        .panes
        .iter()
        .map(restore_pane)
        .collect::<Result<Vec<_>, _>>()?;
    Window::restore(WindowId::new(window.id), layout, panes)
}

fn restore_pane(pane: &PaneState) -> Result<Pane, String> {
    let tabs = pane
        .tabs
        .iter()
        .map(restore_tab)
        .collect::<Result<Vec<_>, _>>()?;
    Pane::restore(PaneId::new(pane.id), tabs, pane.active, pane.page_rows)
}

fn restore_tab(tab: &TabState) -> Result<Tab, String> {
    let location =
        |text: &String| Location::new(text.clone()).map_err(|e| format!("tab {}: {e}", tab.id));
    let list = |texts: &[String]| texts.iter().map(location).collect::<Result<Vec<_>, _>>();
    let history = History::from_stacks(list(&tab.back)?, list(&tab.forward)?);
    let view = ViewState {
        cursor: tab.cursor,
        scroll: tab.scroll,
    };
    Ok(Tab::restore(
        TabId::new(tab.id),
        location(&tab.location)?,
        history,
        view,
    ))
}
