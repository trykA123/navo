use super::{StateError, Tab};
use crate::PaneId;

#[allow(dead_code, reason = "stubs until the state is implemented")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    id: PaneId,
    tabs: Vec<Tab>,
    active: usize,
}

#[allow(
    unused_variables,
    dead_code,
    reason = "stubs until the state is implemented"
)]
impl Pane {
    #[must_use]
    pub fn id(&self) -> PaneId {
        todo!()
    }

    #[must_use]
    pub fn tabs(&self) -> &[Tab] {
        todo!()
    }

    #[must_use]
    pub fn active_index(&self) -> usize {
        todo!()
    }

    #[must_use]
    pub fn active(&self) -> &Tab {
        todo!()
    }

    pub(crate) fn new(id: PaneId, first: Tab) -> Self {
        todo!()
    }

    pub(crate) fn active_mut(&mut self) -> &mut Tab {
        todo!()
    }

    pub(crate) fn open_tab(&mut self, tab: Tab) {
        todo!()
    }

    pub(crate) fn close_tab(&mut self, index: usize) -> Result<(), StateError> {
        todo!()
    }

    pub(crate) fn activate(&mut self, index: usize) -> Result<(), StateError> {
        todo!()
    }
}
