use super::{AppState, StateError, Tab, ViewState};
use crate::{PaneId, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabMove {
    pub from: PaneId,
    pub index: usize,
    pub to: PaneId,
    pub at: usize,
    pub copy: bool,
}

impl AppState {
    pub fn move_tab(&mut self, change: TabMove) -> Result<TabId, StateError> {
        let TabMove {
            from,
            index,
            to,
            mut at,
            copy,
        } = change;
        for pane in [from, to] {
            if !self.focused.contains(pane) {
                return Err(StateError::UnknownPane(pane));
            }
        }
        let tabs = self.pane(from).ok_or(StateError::UnknownPane(from))?.tabs();
        let source = tabs.get(index).ok_or(StateError::NoSuchTab(index))?;
        let (source_id, is_alone) = (source.id(), tabs.len() == 1);
        let folder = source.base_or_location().clone();
        let copied = copy.then(|| copy_of(source));
        let tab = match copied {
            Some(mut tab) => {
                tab.id = TabId::new(self.take_ids(1));
                tab
            }
            None if from == to && is_alone => {
                self.focus_pane_id(to)?;
                return Ok(source_id);
            }
            None => {
                if from == to && at > index {
                    at -= 1;
                }
                let fresh = is_alone.then(|| self.new_tab(folder));
                let source = self.pane_mut(from).ok_or(StateError::UnknownPane(from))?;
                if let Some(fresh) = fresh {
                    source.insert_tab(1, fresh);
                }
                source.take_tab(index)?
            }
        };
        let id = tab.id();
        self.pane_mut(to)
            .ok_or(StateError::UnknownPane(to))?
            .insert_tab(at, tab);
        self.focus_pane_id(to)?;
        Ok(id)
    }

    fn focus_pane_id(&mut self, pane: PaneId) -> Result<(), StateError> {
        let n = self
            .focused
            .layout()
            .numbered()
            .iter()
            .position(|shown| *shown == pane)
            .ok_or(StateError::UnknownPane(pane))?;
        self.focused.focus_number(n + 1)
    }
}

fn copy_of(source: &Tab) -> Tab {
    let base = source.base_or_location();
    if base == source.location() {
        source.workspace_snapshot()
    } else {
        Tab::restore(
            source.id(),
            base.clone(),
            source.history().clone(),
            ViewState::default(),
        )
    }
}
