use super::tab_move::copy_of;
use super::{AppState, StateError, Tab};
use crate::{Location, PaneId, TabId};

const MAX_CLOSED_TABS: usize = 20;
const MAX_TAB_LABEL: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ClosedTab {
    location: Location,
    label: Option<String>,
    pinned: bool,
    pane: PaneId,
}

impl Tab {
    pub(crate) fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned;
    }

    pub(crate) fn set_label(&mut self, label: Option<String>) {
        self.label = label;
    }
}

pub(crate) fn tab_label(text: &str) -> Result<Option<String>, StateError> {
    let text = text.trim();
    if text.chars().count() > MAX_TAB_LABEL {
        return Err(StateError::InvalidTabLabel);
    }
    Ok((!text.is_empty()).then(|| text.to_owned()))
}

impl AppState {
    pub(crate) fn find_tab(&self, id: TabId) -> Result<(PaneId, usize), StateError> {
        self.focused
            .panes()
            .into_iter()
            .find_map(|pane| Some((pane.id(), pane.position(id)?)))
            .ok_or(StateError::UnknownTab(id))
    }

    pub(crate) fn pin_tab(&mut self, id: TabId, pinned: bool) -> Result<(), StateError> {
        let (pane, _) = self.find_tab(id)?;
        let pane = self.pane_mut(pane).ok_or(StateError::UnknownPane(pane))?;
        pane.tab_mut(id)
            .ok_or(StateError::UnknownTab(id))?
            .set_pinned(pinned);
        pane.keep_pinned_first();
        Ok(())
    }

    pub(crate) fn rename_tab(&mut self, id: TabId, label: &str) -> Result<(), StateError> {
        let label = tab_label(label)?;
        self.find_tab(id)?;
        self.tab_mut(id)
            .ok_or(StateError::UnknownTab(id))?
            .set_label(label);
        Ok(())
    }

    pub(crate) fn duplicate_tab(&mut self, id: TabId) -> Result<TabId, StateError> {
        let (pane, index) = self.find_tab(id)?;
        let mut copy = copy_of(self.tab(id).ok_or(StateError::UnknownTab(id))?);
        copy.id = TabId::new(self.take_ids(1));
        let copied = copy.id();
        self.pane_mut(pane)
            .ok_or(StateError::UnknownPane(pane))?
            .insert_tab(index + 1, copy);
        self.focus_pane_id(pane)?;
        Ok(copied)
    }

    pub(crate) fn close_tab(&mut self, id: TabId) -> Result<(), StateError> {
        let (pane, _) = self.find_tab(id)?;
        let is_only_tab = self.focused.panes().len() == 1
            && self.pane(pane).is_some_and(|pane| pane.tabs().len() == 1);
        if is_only_tab {
            return Err(StateError::LastTab);
        }
        let tab = self.detach_tab(id)?;
        self.remember_closed(pane, &tab);
        Ok(())
    }

    pub(crate) fn close_other_tabs(&mut self, id: TabId) -> Result<(), StateError> {
        self.close_tabs_where(id, |_, tab| tab.id() != id)?;
        let (pane, index) = self.find_tab(id)?;
        self.pane_mut(pane)
            .ok_or(StateError::UnknownPane(pane))?
            .activate(index)
    }

    pub(crate) fn close_tabs_right(&mut self, id: TabId) -> Result<(), StateError> {
        let (_, index) = self.find_tab(id)?;
        self.close_tabs_where(id, |at, _| at > index)
    }

    pub(crate) fn reopen_tab(&mut self) -> Result<TabId, StateError> {
        let closed = self
            .focused
            .closed
            .pop_back()
            .ok_or(StateError::NothingToReopen)?;
        let pane = if self.focused.contains(closed.pane) {
            closed.pane
        } else {
            self.focused_pane()
        };
        let mut tab = self.new_tab(closed.location);
        tab.set_pinned(closed.pinned);
        tab.set_label(closed.label);
        let id = tab.id();
        let target = self.pane_mut(pane).ok_or(StateError::UnknownPane(pane))?;
        target.insert_tab(target.tabs().len(), tab);
        self.focus_pane_id(pane)?;
        Ok(id)
    }

    pub(super) fn detach_tab(&mut self, id: TabId) -> Result<Tab, StateError> {
        let (pane, index) = self.find_tab(id)?;
        let source = self.pane(pane).ok_or(StateError::UnknownPane(pane))?;
        let is_alone = source.tabs().len() == 1;
        if is_alone {
            let folder = source.active().base_or_location().clone();
            let fresh = self.new_tab(folder);
            self.pane_mut(pane)
                .ok_or(StateError::UnknownPane(pane))?
                .insert_tab(1, fresh);
        }
        let tab = self
            .pane_mut(pane)
            .ok_or(StateError::UnknownPane(pane))?
            .take_tab(index)?;
        if is_alone && self.focused.panes().len() > 1 {
            self.focused.close_discarding_tabs(pane)?;
        }
        Ok(tab)
    }

    pub(super) fn remember_closed(&mut self, pane: PaneId, tab: &Tab) {
        let closed = &mut self.focused.closed;
        if closed.len() == MAX_CLOSED_TABS {
            closed.pop_front();
        }
        closed.push_back(ClosedTab {
            location: tab.base_or_location().clone(),
            label: tab.label.clone(),
            pinned: tab.pinned,
            pane,
        });
    }

    fn close_tabs_where(
        &mut self,
        id: TabId,
        is_closing: impl Fn(usize, &Tab) -> bool,
    ) -> Result<(), StateError> {
        let (pane_id, _) = self.find_tab(id)?;
        let pane = self
            .pane_mut(pane_id)
            .ok_or(StateError::UnknownPane(pane_id))?;
        let active = pane.active().id();
        let closing: Vec<TabId> = (pane.tabs().iter().enumerate())
            .filter(|(at, tab)| !tab.is_pinned() && is_closing(*at, tab))
            .map(|(_, tab)| tab.id())
            .collect();
        let mut closed = Vec::with_capacity(closing.len());
        for tab in closing {
            if let Some(at) = pane.position(tab) {
                closed.push(pane.take_tab(at)?);
            }
        }
        let shown = pane.position(active).or_else(|| pane.position(id));
        pane.activate(shown.unwrap_or(0))?;
        for tab in &closed {
            self.remember_closed(pane_id, tab);
        }
        Ok(())
    }
}
