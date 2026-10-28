use rusty_core::{Location, OpKind, TabId, is_protected};
use rusty_fs::volume_of;

use super::{Browser, DropConfirm, DropRequest, OpRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteRefusal {
    Nothing,
    AlreadyHere,
    IntoItself,
    IntoResults,
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
    let parents: Vec<Location> = sources.iter().filter_map(Location::parent).collect();
    refusal_from(target, (&parents, sources))
}

#[must_use]
pub fn refusal_from(
    target: &Location,
    (parents, folders): (&[Location], &[Location]),
) -> Option<PasteRefusal> {
    if crate::browser::find::is_results(target) {
        return Some(PasteRefusal::IntoResults);
    }
    if parents.iter().any(|parent| parent.same_place(target)) {
        return Some(PasteRefusal::AlreadyHere);
    }
    std::iter::successors(Some(target.clone()), Location::parent)
        .any(|place| folders.iter().any(|folder| folder.same_place(&place)))
        .then_some(PasteRefusal::IntoItself)
}

impl Browser {
    #[must_use]
    pub fn operate_refusal(&self, kind: OpKind) -> Option<PasteRefusal> {
        let targets = matches!(kind, OpKind::Copy | OpKind::Move | OpKind::Link);
        self.other_location()
            .filter(|target| targets && crate::browser::find::is_results(target))
            .map(|_| PasteRefusal::IntoResults)
    }

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
            structure: None,
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
