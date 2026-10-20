use rusty_core::{Location, OpKind, TabId, is_protected};
use rusty_fs::volume_of;

use super::{Browser, DropConfirm, DropRequest, OpRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteRefusal {
    Nothing,
    AlreadyHere,
    IntoItself,
}

#[must_use]
pub fn same_volume(a: &Location, b: &Location) -> bool {
    volume_of(a) == volume_of(b)
}

#[must_use]
pub fn drop_refusal(target: &Location, sources: &[Location]) -> Option<PasteRefusal> {
    if sources.is_empty() {
        return Some(PasteRefusal::Nothing);
    }
    if sources
        .iter()
        .any(|source| source.parent().as_ref() == Some(target))
    {
        return Some(PasteRefusal::AlreadyHere);
    }
    let mut current = Some(target.clone());
    while let Some(location) = current {
        if sources.contains(&location) {
            return Some(PasteRefusal::IntoItself);
        }
        current = location.parent();
    }
    None
}

impl Browser {
    pub fn paste(&mut self, kind: OpKind, sources: Vec<Location>) -> Result<u64, PasteRefusal> {
        self.drop_into(DropRequest {
            kind,
            sources,
            target: self.state.active_tab().location().clone(),
            confirm: DropConfirm::Never,
        })
    }

    pub fn drop_into(&mut self, request: DropRequest) -> Result<u64, PasteRefusal> {
        let DropRequest {
            kind,
            sources,
            target,
            confirm,
        } = request;
        if let Some(refused) = drop_refusal(&target, &sources) {
            return Err(refused);
        }
        let source_tab = self.drop_source_tab(&sources);
        let same_volume = sources
            .first()
            .is_some_and(|source| same_volume(source, &target));
        let asks = confirm == DropConfirm::Always
            || (confirm == DropConfirm::Moves && kind == OpKind::Move)
            || is_protected(&target, &self.operations.protected);
        self.next_op += 1;
        let request = OpRequest {
            id: self.next_op,
            kind,
            sources,
            target: Some(target),
            same_volume,
            protected: self.operations.protected.clone(),
            source_tab,
            asks,
        };
        let id = request.id;
        if self.submit_plan(request) {
            Ok(id)
        } else {
            Err(PasteRefusal::Nothing)
        }
    }

    fn drop_source_tab(&self, sources: &[Location]) -> TabId {
        let parent = sources.first().and_then(Location::parent);
        self.state
            .window_ids()
            .into_iter()
            .filter_map(|id| self.state.window(id))
            .flat_map(|window| window.panes())
            .flat_map(|pane| pane.tabs())
            .find(|tab| Some(tab.location()) == parent.as_ref())
            .map_or_else(|| self.state.active_tab().id(), |tab| tab.id())
    }
}
